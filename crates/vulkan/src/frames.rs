use ash::{Device, khr, vk};
use winit::dpi::PhysicalSize;

use crate::context::Context;
use crate::window::{Error, vk_error};

pub(crate) enum Frame {
    Presented { suboptimal: bool },
    Outdated,
}

pub(crate) struct Frames {
    device: Device,
    loader: khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    pool: vk::CommandPool,
    commands: vk::CommandBuffer,
    acquired: vk::Semaphore,
    rendered: Vec<vk::Semaphore>,
}

impl Frames {
    pub(crate) fn new(context: &Context, size: PhysicalSize<u32>) -> Result<Option<Self>, Error> {
        if size.width == 0 || size.height == 0 {
            return Ok(None);
        }
        let caps = context.capabilities()?;
        let extent = if caps.current_extent.width == u32::MAX {
            vk::Extent2D {
                width: size
                    .width
                    .clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                height: size
                    .height
                    .clamp(caps.min_image_extent.height, caps.max_image_extent.height),
            }
        } else {
            caps.current_extent
        };
        if extent.width == 0 || extent.height == 0 {
            return Ok(None);
        }
        let formats = context.formats()?;
        let mut format = formats
            .iter()
            .copied()
            .find(|format| {
                format.format == vk::Format::B8G8R8A8_SRGB
                    && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .or_else(|| formats.first().copied())
            .ok_or("surface advertised no formats")?;
        if format.format == vk::Format::UNDEFINED {
            format.format = vk::Format::B8G8R8A8_UNORM;
        }
        let alpha = [
            vk::CompositeAlphaFlagsKHR::OPAQUE,
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::INHERIT,
        ]
        .into_iter()
        .find(|flag| caps.supported_composite_alpha.contains(*flag))
        .ok_or("surface advertised no composite alpha mode")?;
        let mut count = caps.min_image_count.saturating_add(1);
        if caps.max_image_count != 0 {
            count = count.min(caps.max_image_count);
        }
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(context.surface())
            .min_image_count(count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::TRANSFER_DST)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(alpha)
            .present_mode(vk::PresentModeKHR::FIFO)
            .clipped(true);
        let mut frames = Self {
            device: context.device.clone(),
            loader: context.swapchain_loader(),
            swapchain: vk::SwapchainKHR::null(),
            images: Vec::new(),
            pool: vk::CommandPool::null(),
            commands: vk::CommandBuffer::null(),
            acquired: vk::Semaphore::null(),
            rendered: Vec::new(),
        };
        // SAFETY: The surface, graphics queue family and device have been checked for this
        // configuration. Frames owns all created handles, including during partial failure.
        unsafe {
            frames.swapchain = frames
                .loader
                .create_swapchain(&info, None)
                .map_err(vk_error("create swapchain"))?;
            frames.images = frames
                .loader
                .get_swapchain_images(frames.swapchain)
                .map_err(vk_error("get swapchain images"))?;
            let info = vk::CommandPoolCreateInfo::default().queue_family_index(context.family);
            frames.pool = frames
                .device
                .create_command_pool(&info, None)
                .map_err(vk_error("create command pool"))?;
            let info = vk::CommandBufferAllocateInfo::default()
                .command_pool(frames.pool)
                .command_buffer_count(1);
            frames.commands = frames
                .device
                .allocate_command_buffers(&info)
                .map_err(vk_error("allocate command buffer"))?[0];
            let semaphore = vk::SemaphoreCreateInfo::default();
            frames.acquired = frames
                .device
                .create_semaphore(&semaphore, None)
                .map_err(vk_error("create acquire semaphore"))?;
            for _ in &frames.images {
                frames.rendered.push(
                    frames
                        .device
                        .create_semaphore(&semaphore, None)
                        .map_err(vk_error("create present semaphore"))?,
                );
            }
        }
        Ok(Some(frames))
    }

    pub(crate) fn clear_and_present(&self, context: &Context) -> Result<Frame, Error> {
        let device = &self.device;
        // SAFETY: The UI thread owns the queue and swapchain. The prior submit completed before this
        // call, and each presentation semaphore is reused only after its image is acquired again.
        unsafe {
            let (index, acquire_suboptimal) = match self.loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.acquired,
                vk::Fence::null(),
            ) {
                Ok(image) => image,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(Frame::Outdated),
                Err(error) => return Err(vk_error("acquire image")(error)),
            };
            let image = self.images[index as usize];
            device
                .reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty())
                .map_err(vk_error("reset command pool"))?;
            let begin = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            device
                .begin_command_buffer(self.commands, &begin)
                .map_err(vk_error("begin clear commands"))?;

            let range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
                .layer_count(1);
            let barrier = vk::ImageMemoryBarrier::default()
                .image(image)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .subresource_range(range);
            let to_clear = barrier
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
            device.cmd_pipeline_barrier(
                self.commands,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_clear],
            );
            let color = vk::ClearColorValue {
                float32: [0.035, 0.075, 0.11, 1.0],
            };
            device.cmd_clear_color_image(
                self.commands,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &color,
                &[range],
            );
            let to_present = barrier
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE);
            device.cmd_pipeline_barrier(
                self.commands,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_present],
            );
            device
                .end_command_buffer(self.commands)
                .map_err(vk_error("end clear commands"))?;

            let image_ready = [self.acquired];
            let render_done = [self.rendered[index as usize]];
            let submit = vk::SubmitInfo::default()
                .wait_semaphores(&image_ready)
                .wait_dst_stage_mask(&[vk::PipelineStageFlags::TRANSFER])
                .command_buffers(std::slice::from_ref(&self.commands))
                .signal_semaphores(&render_done);
            device
                .queue_submit(context.queue, &[submit], vk::Fence::null())
                .map_err(vk_error("submit clear"))?;
            let present = vk::PresentInfoKHR::default()
                .wait_semaphores(&render_done)
                .swapchains(std::slice::from_ref(&self.swapchain))
                .image_indices(std::slice::from_ref(&index));
            let presented = self.loader.queue_present(context.queue, &present);
            device
                .queue_wait_idle(context.queue)
                .map_err(vk_error("wait for clear submission"))?;
            match presented {
                Ok(suboptimal) => Ok(Frame::Presented {
                    suboptimal: acquire_suboptimal || suboptimal,
                }),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(Frame::Outdated),
                Err(error) => Err(vk_error("present clear")(error)),
            }
        }
    }
}

impl Drop for Frames {
    fn drop(&mut self) {
        // SAFETY: The swapchain shares a live Context and the queue is no longer in use by the UI.
        unsafe {
            let _ = self.device.device_wait_idle();
            for semaphore in self.rendered.drain(..) {
                self.device.destroy_semaphore(semaphore, None);
            }
            if self.acquired != vk::Semaphore::null() {
                self.device.destroy_semaphore(self.acquired, None);
            }
            if self.pool != vk::CommandPool::null() {
                self.device.destroy_command_pool(self.pool, None);
            }
            if self.swapchain != vk::SwapchainKHR::null() {
                self.loader.destroy_swapchain(self.swapchain, None);
            }
        }
    }
}
