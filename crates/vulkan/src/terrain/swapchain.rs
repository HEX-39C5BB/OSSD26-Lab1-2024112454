use std::sync::Arc;

use ash::{khr, vk};
use rustcraft_scene::draw::Frame;

use super::gpu::{Gpu, Image, view};
use super::render::{Flight, Pipelines, Target};
use crate::window::{Error, vk_error};

const FRAMES: usize = 2;

pub(super) struct WindowRenderer {
    swapchain: Option<Swapchain>,
    flights: [Flight; FRAMES],
    gpu: Arc<Gpu>,
    next: usize,
}

impl WindowRenderer {
    pub fn graphics(&self) -> &Arc<super::GraphicsInfo> {
        &self.gpu.info
    }

    pub fn surface_format(&self) -> Option<&Arc<str>> {
        self.swapchain
            .as_ref()
            .map(|swapchain| &swapchain.format_label)
    }
    pub fn new(gpu: Arc<Gpu>) -> Result<Self, Error> {
        let flights = [
            Flight::new(Arc::clone(&gpu), true)?,
            Flight::new(Arc::clone(&gpu), true)?,
        ];
        Ok(Self {
            swapchain: None,
            flights,
            gpu,
            next: 0,
        })
    }

    pub fn resize(&mut self) {
        self.swapchain = None;
    }

    pub fn draw(&mut self, size: [u32; 2], frame: &Frame) -> Result<Option<[u32; 2]>, Error> {
        if size.contains(&0) {
            return Ok(None);
        }
        if self.swapchain.is_none() {
            self.swapchain = Swapchain::new(Arc::clone(&self.gpu), size)?;
        }
        let Some(swapchain) = &mut self.swapchain else {
            return Ok(None);
        };
        swapchain.pipelines.prepare_atlas(frame)?;
        let slot = self.next;
        let flight = &mut self.flights[slot];
        flight.wait()?;
        // This slot's fence guarantees that its image-available semaphore can be reused.
        let acquired = unsafe {
            swapchain.loader.acquire_next_image(
                swapchain.raw,
                16_000_000,
                flight.available,
                vk::Fence::null(),
            )
        };
        let (index, resize) = match acquired {
            Ok((index, resize)) => (index, resize),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.resize();
                return Ok(None);
            }
            Err(vk::Result::TIMEOUT | vk::Result::NOT_READY) => return Ok(None),
            Err(error) => return Err(vk_error("acquire terrain swapchain image")(error)),
        };
        let i = index as usize;
        let frame_done = swapchain.ready[i];
        flight.begin(frame)?;
        swapchain.pipelines.record(
            flight,
            Target {
                color: swapchain.images[i],
                view: swapchain.views[i],
                depth: &swapchain.depth[slot],
                extent: swapchain.extent,
            },
            frame,
            true,
        );
        flight.submit(Some(frame_done))?;
        self.next = (slot + 1) % FRAMES;
        let waits = [frame_done];
        let chains = [swapchain.raw];
        let indices = [index];
        let info = vk::PresentInfoKHR::default()
            .wait_semaphores(&waits)
            .swapchains(&chains)
            .image_indices(&indices);
        // A per-image semaphore is reused only after that image is acquired again.
        let presented = unsafe { swapchain.loader.queue_present(self.gpu.queue, &info) };
        match presented {
            Ok(suboptimal) => {
                let size = [swapchain.extent.width, swapchain.extent.height];
                if suboptimal || resize {
                    self.resize();
                }
                Ok(Some(size))
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.resize();
                Ok(None)
            }
            Err(error) => Err(vk_error("present terrain")(error)),
        }
    }
}

struct Swapchain {
    format_label: Arc<str>,
    gpu: Arc<Gpu>,
    loader: khr::swapchain::Device,
    raw: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    ready: Vec<vk::Semaphore>,
    depth: [Image; FRAMES],
    pipelines: Pipelines,
    extent: vk::Extent2D,
}

impl Swapchain {
    fn new(gpu: Arc<Gpu>, [width, height]: [u32; 2]) -> Result<Option<Self>, Error> {
        let surface = gpu.surface.as_ref().expect("desktop GPU surface");
        let caps = unsafe {
            surface
                .loader
                .get_physical_device_surface_capabilities(gpu.physical, surface.raw)
        }
        .map_err(vk_error("query terrain surface capabilities"))?;
        if !caps
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        {
            return Err("window surface does not support terrain color rendering".into());
        }

        let extent = if caps.current_extent.width == u32::MAX {
            vk::Extent2D {
                width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
            }
        } else {
            caps.current_extent
        };
        if extent.width == 0 || extent.height == 0 {
            return Ok(None);
        }
        let formats = unsafe {
            surface
                .loader
                .get_physical_device_surface_formats(gpu.physical, surface.raw)
        }
        .map_err(vk_error("query terrain surface formats"))?;
        let mut format = formats
            .iter()
            .copied()
            .find(|format| {
                [vk::Format::B8G8R8A8_UNORM, vk::Format::R8G8B8A8_UNORM].contains(&format.format)
                    && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .or_else(|| formats.first().copied())
            .ok_or("window surface has no format")?;
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
        .find(|&mode| caps.supported_composite_alpha.contains(mode))
        .ok_or("window surface has no composite alpha mode")?;
        let count = caps
            .min_image_count
            .saturating_add(1)
            .min(if caps.max_image_count == 0 {
                u32::MAX
            } else {
                caps.max_image_count
            });
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface.raw)
            .min_image_count(count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(alpha)
            .present_mode(vk::PresentModeKHR::FIFO)
            .clipped(true);

        let depth = [
            Image::depth(Arc::clone(&gpu), extent)?,
            Image::depth(Arc::clone(&gpu), extent)?,
        ];
        let pipelines = Pipelines::new(Arc::clone(&gpu), format.format)?;
        let loader = khr::swapchain::Device::new(&gpu.instance.raw, &gpu.device);
        let raw = match unsafe { loader.create_swapchain(&info, None) } {
            Ok(raw) => raw,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(None),
            Err(error) => return Err(vk_error("create terrain swapchain")(error)),
        };
        let mut swapchain = Self {
            format_label: format!("{:?}", format.format).into(),
            gpu,
            loader,
            raw,
            images: Vec::new(),
            views: Vec::new(),
            ready: Vec::new(),
            depth,
            pipelines,
            extent,
        };
        swapchain.images = unsafe { swapchain.loader.get_swapchain_images(raw) }
            .map_err(vk_error("get terrain swapchain images"))?;
        for &image in &swapchain.images {
            swapchain.views.push(view(
                &swapchain.gpu,
                image,
                format.format,
                vk::ImageAspectFlags::COLOR,
            )?);
            swapchain.ready.push(
                unsafe {
                    swapchain
                        .gpu
                        .device
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                }
                .map_err(vk_error("create terrain presentation semaphore"))?,
            );
        }
        Ok(Some(swapchain))
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        // Resize and teardown release resources only after submissions using the old images finish.
        unsafe {
            let _ = self.gpu.device.device_wait_idle();
            for view in self.views.drain(..) {
                self.gpu.device.destroy_image_view(view, None);
            }
            for semaphore in self.ready.drain(..) {
                self.gpu.device.destroy_semaphore(semaphore, None);
            }
            self.loader.destroy_swapchain(self.raw, None);
        }
    }
}
