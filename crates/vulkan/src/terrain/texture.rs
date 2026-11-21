use std::sync::Arc;

use ash::vk;
use rustcraft_scene::assets::TextureAtlas;
use rustcraft_scene::draw::Frame;

use super::gpu::{Buffer, Gpu, Image, range};
use super::render::Flight;
use crate::window::{Error, vk_error};

pub(super) struct TextureBinding {
    gpu: Arc<Gpu>,
    source: Option<Arc<TextureAtlas>>,
    image: Image,
    sampler: vk::Sampler,
    pool: vk::DescriptorPool,
    pub set: vk::DescriptorSet,
}

impl TextureBinding {
    pub fn same_atlas(&self, atlas: Option<&Arc<TextureAtlas>>) -> bool {
        match (&self.source, atlas) {
            (None, None) => true,
            (Some(current), Some(atlas)) => Arc::ptr_eq(current, atlas),
            _ => false,
        }
    }

    pub fn new(
        gpu: Arc<Gpu>,
        layout: vk::DescriptorSetLayout,
        atlas: Option<&Arc<TextureAtlas>>,
    ) -> Result<Self, Error> {
        let (width, height, pixels) = atlas.map_or((1, 1, &[255, 255, 255, 255][..]), |atlas| {
            (atlas.width, atlas.height, atlas.pixels.as_slice())
        });
        let extent = vk::Extent2D { width, height };
        let image = Image::new(
            Arc::clone(&gpu),
            extent,
            vk::Format::R8G8B8A8_UNORM,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            vk::ImageAspectFlags::COLOR,
        )?;
        upload(&gpu, &image, extent, pixels)?;
        let mut binding = Self {
            gpu,
            source: atlas.cloned(),
            image,
            sampler: vk::Sampler::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
        };
        let device = &binding.gpu.device;
        let sampler = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE);
        let sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)];
        let layouts = [layout];
        // The pool retains the set for the lifetime of the image and sampler.
        unsafe {
            binding.sampler = device
                .create_sampler(&sampler, None)
                .map_err(vk_error("create terrain atlas sampler"))?;
            binding.pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(1)
                        .pool_sizes(&sizes),
                    None,
                )
                .map_err(vk_error("create terrain atlas descriptor pool"))?;
            binding.set = device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(binding.pool)
                        .set_layouts(&layouts),
                )
                .map_err(vk_error("allocate terrain atlas descriptor"))?[0];
            let images = [vk::DescriptorImageInfo::default()
                .sampler(binding.sampler)
                .image_view(binding.image.view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(binding.set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&images)],
                &[],
            );
        }
        Ok(binding)
    }
}

impl Drop for TextureBinding {
    fn drop(&mut self) {
        // The owning renderer has completed all draws that use the atlas.
        unsafe {
            self.gpu.device.destroy_descriptor_pool(self.pool, None);
            self.gpu.device.destroy_sampler(self.sampler, None);
        }
    }
}

fn upload(gpu: &Arc<Gpu>, image: &Image, extent: vk::Extent2D, pixels: &[u8]) -> Result<(), Error> {
    let staging = Buffer::new(
        Arc::clone(gpu),
        pixels.len(),
        vk::BufferUsageFlags::TRANSFER_SRC,
    )?;
    staging.upload(pixels, &[])?;
    let mut flight = Flight::new(Arc::clone(gpu), false)?;
    flight.begin(&Frame {
        terrain: Vec::new(),
        hud: Vec::new(),
        view_projection: [[0.0; 4]; 4],
        atlas: None,
        background: [0.0; 4],
    })?;
    let start = vk::ImageMemoryBarrier2::default()
        .dst_stage_mask(vk::PipelineStageFlags2::COPY)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .image(image.raw)
        .subresource_range(range(vk::ImageAspectFlags::COLOR))
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED);
    let finish = start
        .src_stage_mask(vk::PipelineStageFlags2::COPY)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
        .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
    let copy = vk::BufferImageCopy::default()
        .image_subresource(
            vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1),
        )
        .image_extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        });
    // The one-time copy transitions the image for sampling before staging memory is released.
    unsafe {
        let device = &gpu.device;
        device.cmd_pipeline_barrier2(
            flight.command,
            &vk::DependencyInfo::default().image_memory_barriers(&[start]),
        );
        device.cmd_copy_buffer_to_image(
            flight.command,
            staging.raw,
            image.raw,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[copy],
        );
        device.cmd_pipeline_barrier2(
            flight.command,
            &vk::DependencyInfo::default().image_memory_barriers(&[finish]),
        );
    }
    flight.submit(None)?;
    flight.wait()
}
