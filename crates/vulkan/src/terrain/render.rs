use std::io::Cursor;
use std::sync::Arc;

use ash::{util::read_spv, vk};
use rustcraft_scene::draw::{Frame, Vertex};

use super::gpu::{Buffer, Gpu, Image, range};
use super::texture::TextureBinding;
use crate::window::{Error, vk_error};

const VERTEX_SHADER: &[u8] = include_bytes!("shaders/terrain.vert.spv");
const FRAGMENT_SHADER: &[u8] = include_bytes!("shaders/terrain.frag.spv");

pub(super) struct Pipelines {
    gpu: Arc<Gpu>,
    layout: vk::PipelineLayout,
    terrain: vk::Pipeline,
    hud: vk::Pipeline,
    texture_layout: vk::DescriptorSetLayout,
    texture: Option<TextureBinding>,
}

impl Pipelines {
    pub fn new(gpu: Arc<Gpu>, format: vk::Format) -> Result<Self, Error> {
        let mut pipelines = Self {
            gpu,
            layout: vk::PipelineLayout::null(),
            terrain: vk::Pipeline::null(),
            hud: vk::Pipeline::null(),
            texture_layout: vk::DescriptorSetLayout::null(),
            texture: None,
        };
        let constants = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .size(size_of::<[[f32; 4]; 4]>() as u32)];
        // Push constants contain the terrain camera transform.
        unsafe {
            let bindings = [vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
            pipelines.texture_layout = pipelines
                .gpu
                .device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .map_err(vk_error("create terrain atlas layout"))?;
            let layouts = [pipelines.texture_layout];
            let info = vk::PipelineLayoutCreateInfo::default()
                .push_constant_ranges(&constants)
                .set_layouts(&layouts);
            pipelines.layout = pipelines
                .gpu
                .device
                .create_pipeline_layout(&info, None)
                .map_err(vk_error("create terrain pipeline layout"))?;
        }
        {
            let vertex = Shader::new(&pipelines.gpu, VERTEX_SHADER)?;
            let fragment = Shader::new(&pipelines.gpu, FRAGMENT_SHADER)?;
            pipelines.terrain = pipelines.create(format, &vertex, &fragment, true)?;
            pipelines.hud = pipelines.create(format, &vertex, &fragment, false)?;
        }
        Ok(pipelines)
    }

    pub fn prepare_atlas(&mut self, frame: &Frame) -> Result<(), Error> {
        let atlas = frame.atlas.as_ref();
        if self
            .texture
            .as_ref()
            .is_some_and(|texture| texture.same_atlas(atlas))
        {
            return Ok(());
        }
        if self.texture.is_some() {
            // Atlas replacement is infrequent and must wait for all in-flight users.
            unsafe { self.gpu.device.queue_wait_idle(self.gpu.queue) }
                .map_err(vk_error("finish draws using terrain atlas"))?;
        }
        self.texture = Some(TextureBinding::new(
            Arc::clone(&self.gpu),
            self.texture_layout,
            atlas,
        )?);
        Ok(())
    }

    fn create(
        &self,
        format: vk::Format,
        vert: &Shader<'_>,
        frag: &Shader<'_>,
        depth: bool,
    ) -> Result<vk::Pipeline, Error> {
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vert.raw)
                .name(c"main"),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(frag.raw)
                .name(c"main"),
        ];
        let bindings = [vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)];
        let attributes = [
            vk::VertexInputAttributeDescription::default()
                .location(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(std::mem::offset_of!(Vertex, pos) as u32),
            vk::VertexInputAttributeDescription::default()
                .location(1)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(std::mem::offset_of!(Vertex, color) as u32),
            vk::VertexInputAttributeDescription::default()
                .location(2)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(std::mem::offset_of!(Vertex, uv) as u32),
            vk::VertexInputAttributeDescription::default()
                .location(3)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(std::mem::offset_of!(Vertex, atlas) as u32),
        ];
        let input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&bindings)
            .vertex_attribute_descriptions(&attributes);
        let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .line_width(1.0);
        let samples = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let depth = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(depth)
            .depth_write_enable(depth)
            .depth_compare_op(vk::CompareOp::LESS);
        let attachments = [vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)];
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
        let dynamic = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);
        let formats = [format];
        let mut rendering = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&formats)
            .depth_attachment_format(self.gpu.depth_format);
        let info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&input)
            .input_assembly_state(&assembly)
            .viewport_state(&viewport)
            .rasterization_state(&raster)
            .multisample_state(&samples)
            .depth_stencil_state(&depth)
            .color_blend_state(&blend)
            .dynamic_state(&dynamic)
            .layout(self.layout)
            .push_next(&mut rendering);

        // The create info references live shaders and supported color/depth formats.
        unsafe {
            match self.gpu.device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[info],
                None,
            ) {
                Ok(pipelines) => Ok(pipelines[0]),
                Err((pipelines, error)) => {
                    for pipeline in pipelines {
                        self.gpu.device.destroy_pipeline(pipeline, None);
                    }
                    Err(vk_error("create dynamic terrain pipeline")(error))
                }
            }
        }
    }

    pub fn record(&self, flight: &Flight, target: Target<'_>, frame: &Frame, present: bool) {
        let device = &self.gpu.device;
        let command = flight.command;
        let stage = vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
        let depth_stages = vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
            | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS;
        let color_start = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(if present {
                stage
            } else {
                vk::PipelineStageFlags2::NONE
            })
            .dst_stage_mask(stage)
            .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .image(target.color)
            .subresource_range(range(vk::ImageAspectFlags::COLOR))
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED);
        let depth_start = vk::ImageMemoryBarrier2::default()
            .dst_stage_mask(depth_stages)
            .dst_access_mask(
                vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
            )
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .image(target.depth.raw)
            .subresource_range(range(vk::ImageAspectFlags::DEPTH))
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED);
        let area = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: target.extent,
        };
        let color = [vk::RenderingAttachmentInfo::default()
            .image_view(target.view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: frame.background,
                },
            })];
        let depth = vk::RenderingAttachmentInfo::default()
            .image_view(target.depth.view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            });
        let rendering = vk::RenderingInfo::default()
            .color_attachments(&color)
            .depth_attachment(&depth)
            .render_area(area)
            .layer_count(1);
        let viewport = vk::Viewport::default()
            .width(target.extent.width as f32)
            .height(target.extent.height as f32)
            .max_depth(1.0);

        // The flight owns these commands and vertices until its fence signals. Its depth
        // image is private to the flight. Window color images are acquired before use.
        unsafe {
            device.cmd_pipeline_barrier2(
                command,
                &vk::DependencyInfo::default().image_memory_barriers(&[color_start, depth_start]),
            );
            device.cmd_begin_rendering(command, &rendering);
            device.cmd_bind_descriptor_sets(
                command,
                vk::PipelineBindPoint::GRAPHICS,
                self.layout,
                0,
                &[self.texture.as_ref().expect("prepared terrain atlas").set],
                &[],
            );
            device.cmd_set_viewport(command, 0, &[viewport]);
            device.cmd_set_scissor(command, 0, &[area]);
            device.cmd_push_constants(
                command,
                self.layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                bytemuck::bytes_of(&frame.view_projection),
            );
            if let Some(vertices) = &flight.vertices {
                device.cmd_bind_vertex_buffers(command, 0, &[vertices.raw], &[0]);
                if !frame.terrain.is_empty() {
                    device.cmd_bind_pipeline(
                        command,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.terrain,
                    );
                    device.cmd_draw(command, frame.terrain.len() as u32, 1, 0, 0);
                }
                if !frame.hud.is_empty() {
                    device.cmd_bind_pipeline(command, vk::PipelineBindPoint::GRAPHICS, self.hud);
                    device.cmd_draw(
                        command,
                        frame.hud.len() as u32,
                        1,
                        frame.terrain.len() as u32,
                        0,
                    );
                }
            }
            device.cmd_end_rendering(command);
            let finish = color_start
                .src_stage_mask(stage)
                .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                .dst_stage_mask(if present {
                    vk::PipelineStageFlags2::NONE
                } else {
                    vk::PipelineStageFlags2::COPY
                })
                .dst_access_mask(if present {
                    vk::AccessFlags2::NONE
                } else {
                    vk::AccessFlags2::TRANSFER_READ
                })
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(if present {
                    vk::ImageLayout::PRESENT_SRC_KHR
                } else {
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL
                });
            device.cmd_pipeline_barrier2(
                command,
                &vk::DependencyInfo::default().image_memory_barriers(&[finish]),
            );
        }
    }
}

pub(super) struct Target<'a> {
    pub color: vk::Image,
    pub view: vk::ImageView,
    pub depth: &'a Image,
    pub extent: vk::Extent2D,
}

impl Drop for Pipelines {
    fn drop(&mut self) {
        self.texture.take();
        unsafe {
            self.gpu.device.destroy_pipeline(self.hud, None);
            self.gpu.device.destroy_pipeline(self.terrain, None);
            self.gpu.device.destroy_pipeline_layout(self.layout, None);
            self.gpu
                .device
                .destroy_descriptor_set_layout(self.texture_layout, None);
        }
    }
}

struct Shader<'a> {
    gpu: &'a Gpu,
    raw: vk::ShaderModule,
}

impl<'a> Shader<'a> {
    fn new(gpu: &'a Gpu, bytes: &[u8]) -> Result<Self, Error> {
        let code = read_spv(&mut Cursor::new(bytes))?;
        let info = vk::ShaderModuleCreateInfo::default().code(&code);
        let raw = unsafe { gpu.device.create_shader_module(&info, None) }
            .map_err(vk_error("create terrain shader"))?;
        Ok(Self { gpu, raw })
    }
}

impl Drop for Shader<'_> {
    fn drop(&mut self) {
        unsafe { self.gpu.device.destroy_shader_module(self.raw, None) };
    }
}

pub(super) struct Flight {
    gpu: Arc<Gpu>,
    vertices: Option<Buffer>,
    pool: vk::CommandPool,
    pub command: vk::CommandBuffer,
    fence: vk::Fence,
    pub available: vk::Semaphore,
    pending: bool,
}

impl Flight {
    pub fn new(gpu: Arc<Gpu>, window: bool) -> Result<Self, Error> {
        let mut flight = Self {
            gpu,
            vertices: None,
            pool: vk::CommandPool::null(),
            command: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            available: vk::Semaphore::null(),
            pending: false,
        };
        let device = &flight.gpu.device;
        let info = vk::CommandPoolCreateInfo::default().queue_family_index(flight.gpu.family);
        unsafe {
            flight.pool = device
                .create_command_pool(&info, None)
                .map_err(vk_error("create frame command pool"))?;
            let info = vk::CommandBufferAllocateInfo::default()
                .command_pool(flight.pool)
                .command_buffer_count(1);
            flight.command = device
                .allocate_command_buffers(&info)
                .map_err(vk_error("allocate frame commands"))?[0];
            flight.fence = device
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .map_err(vk_error("create frame fence"))?;
            if window {
                flight.available = device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                    .map_err(vk_error("create image-ready semaphore"))?;
            }
        }
        Ok(flight)
    }

    pub fn wait(&mut self) -> Result<(), Error> {
        if self.pending {
            // Only the fence for this flight's last submission can protect its buffers.
            unsafe {
                self.gpu
                    .device
                    .wait_for_fences(&[self.fence], true, u64::MAX)
            }
            .map_err(vk_error("wait for frame fence"))?;
            self.pending = false;
        }
        Ok(())
    }

    pub fn begin(&mut self, frame: &Frame) -> Result<(), Error> {
        self.wait()?;
        let count = frame
            .terrain
            .len()
            .checked_add(frame.hud.len())
            .ok_or("vertex count overflow")?;
        u32::try_from(count).map_err(|_| "too many terrain vertices")?;
        let terrain = bytemuck::cast_slice(&frame.terrain);
        let hud = bytemuck::cast_slice(&frame.hud);
        let size = terrain.len() + hud.len();
        if size > 0 {
            if self
                .vertices
                .as_ref()
                .is_none_or(|buffer| buffer.size < size)
            {
                self.vertices = Some(Buffer::new(
                    Arc::clone(&self.gpu),
                    size.next_power_of_two(),
                    vk::BufferUsageFlags::VERTEX_BUFFER,
                )?);
            }
            self.vertices
                .as_ref()
                .expect("allocated vertices")
                .upload(terrain, hud)?;
        }
        let device = &self.gpu.device;
        unsafe {
            device
                .reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty())
                .map_err(vk_error("reset frame command pool"))?;
            let info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            device
                .begin_command_buffer(self.command, &info)
                .map_err(vk_error("begin frame commands"))
        }
    }

    pub fn submit(&mut self, rendered: Option<vk::Semaphore>) -> Result<(), Error> {
        let device = &self.gpu.device;
        let waits = rendered.map(|_| {
            vk::SemaphoreSubmitInfo::default()
                .semaphore(self.available)
                .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        });
        let signals = rendered.map(|semaphore| {
            vk::SemaphoreSubmitInfo::default()
                .semaphore(semaphore)
                .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        });
        let buffers = [vk::CommandBufferSubmitInfo::default().command_buffer(self.command)];
        let info = vk::SubmitInfo2::default()
            .wait_semaphore_infos(waits.as_slice())
            .signal_semaphore_infos(signals.as_slice())
            .command_buffer_infos(&buffers);
        unsafe {
            device
                .end_command_buffer(self.command)
                .map_err(vk_error("end frame commands"))?;
            device
                .reset_fences(&[self.fence])
                .map_err(vk_error("reset frame fence"))?;
            device
                .queue_submit2(self.gpu.queue, &[info], self.fence)
                .map_err(vk_error("submit Vulkan 1.3 frame"))?;
        }
        self.pending = true;
        Ok(())
    }
}

impl Drop for Flight {
    fn drop(&mut self) {
        let _ = self.wait();
        unsafe {
            self.gpu.device.destroy_semaphore(self.available, None);
            self.gpu.device.destroy_fence(self.fence, None);
            self.gpu.device.destroy_command_pool(self.pool, None);
        }
    }
}
