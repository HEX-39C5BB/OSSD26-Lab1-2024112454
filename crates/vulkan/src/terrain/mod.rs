mod desktop;
mod gpu;
mod render;
mod swapchain;
mod texture;

use std::sync::Arc;
use std::time::Instant;

use ash::vk;
use rustcraft_scene::draw::Frame;
use winit::window::Window;

use crate::window::{Error, vk_error};

pub use desktop::{
    ConsoleKey, Controls, DesktopFrame, FrameTiming, Interaction, MenuKey, PresentedFrame,
    run_terrain, run_terrain_window_benchmark,
};

use gpu::{Buffer, Gpu, Image};
use render::{Flight, Pipelines, Target};

#[derive(Debug)]
pub struct GraphicsInfo {
    pub device: String,
    pub driver: String,
    pub driver_info: String,
    pub vulkan_version: [u32; 3],
    pub queue_family: u32,
}

struct OffscreenTerrain {
    flight: Flight,
    pipelines: Pipelines,
    color: Image,
    depth: Image,
    extent: vk::Extent2D,
}

impl OffscreenTerrain {
    fn new(gpu: &Arc<Gpu>, [width, height]: [u32; 2]) -> Result<Self, Error> {
        if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) {
            return Err("offscreen extent must be between 1 and 4096 pixels per axis".into());
        }
        let extent = vk::Extent2D { width, height };
        let format = vk::Format::R8G8B8A8_UNORM;
        Ok(Self {
            flight: Flight::new(Arc::clone(gpu), false)?,
            pipelines: Pipelines::new(Arc::clone(gpu), format)?,
            color: Image::new(
                Arc::clone(gpu),
                extent,
                format,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
                vk::ImageAspectFlags::COLOR,
            )?,
            depth: Image::depth(Arc::clone(gpu), extent)?,
            extent,
        })
    }

    fn record(&self, frame: &Frame) {
        self.pipelines.record(
            &self.flight,
            Target {
                color: self.color.raw,
                view: self.color.view,
                depth: &self.depth,
                extent: self.extent,
            },
            frame,
            false,
        );
    }
}

/// Renders terrain and HUD through the same dynamic rendering path as the window.
pub fn render_terrain(frame: &Frame, size: [u32; 2]) -> Result<Vec<u8>, Error> {
    let [width, height] = size;
    let gpu = Gpu::new(None)?;
    let device = &gpu.device;
    let mut terrain = OffscreenTerrain::new(&gpu, size)?;
    terrain.pipelines.prepare_atlas(frame)?;
    let output = Buffer::new(
        Arc::clone(&gpu),
        width as usize * height as usize * 4,
        vk::BufferUsageFlags::TRANSFER_DST,
    )?;
    terrain.flight.begin(frame)?;
    terrain.record(frame);

    let copy = vk::BufferImageCopy::default()
        .image_subresource(
            vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1),
        )
        .image_extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        });
    let barrier = vk::BufferMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COPY)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::HOST)
        .dst_access_mask(vk::AccessFlags2::HOST_READ)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(output.raw)
        .size(vk::WHOLE_SIZE);
    // The render pass transitioned the color image for the copy.
    unsafe {
        device.cmd_copy_image_to_buffer(
            terrain.flight.command,
            terrain.color.raw,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            output.raw,
            &[copy],
        );
        device.cmd_pipeline_barrier2(
            terrain.flight.command,
            &vk::DependencyInfo::default().buffer_memory_barriers(&[barrier]),
        );
    }
    terrain.flight.submit(None)?;
    terrain.flight.wait()?;
    output.read()
}

#[derive(Clone, Copy)]
pub enum BenchmarkGpu {
    IntelIntegrated,
    NvidiaDiscrete,
    Auto,
}

impl BenchmarkGpu {
    fn select(self, window: Option<&Window>, timestamps: bool) -> Result<Arc<Gpu>, Error> {
        Gpu::select(window, |device, queue| {
            (!timestamps || queue.timestamp_valid_bits > 0)
                && match self {
                    Self::IntelIntegrated => {
                        device.vendor_id == 0x8086
                            && device.device_type == vk::PhysicalDeviceType::INTEGRATED_GPU
                    }
                    Self::NvidiaDiscrete => {
                        device.vendor_id == 0x10de
                            && device.device_type == vk::PhysicalDeviceType::DISCRETE_GPU
                    }
                    Self::Auto => true,
                }
        })
        .map_err(|error| match self {
            Self::IntelIntegrated => {
                format!("Intel integrated Vulkan benchmark GPU unavailable: {error}").into()
            }
            Self::NvidiaDiscrete => {
                format!("NVIDIA discrete Vulkan benchmark GPU unavailable: {error}").into()
            }
            Self::Auto => format!("Vulkan benchmark GPU unavailable: {error}").into(),
        })
    }
}

pub struct TerrainFrameSample {
    pub prepare_ns: u128,
    pub gpu_ns: u128,
    pub frame_ns: u128,
}

/// Samples the offscreen terrain path with one frame in flight and reusable resources.
pub fn benchmark_terrain(
    size: [u32; 2],
    selection: BenchmarkGpu,
    warmup_frames: usize,
    sample_frames: usize,
    mut build: impl FnMut(usize) -> Frame,
) -> Result<Vec<TerrainFrameSample>, Error> {
    let gpu = selection.select(None, true)?;
    let queries = TimestampQueries::new(Arc::clone(&gpu))?;
    let mut terrain = OffscreenTerrain::new(&gpu, size)?;
    let mut samples = Vec::with_capacity(sample_frames);
    for index in 0..warmup_frames + sample_frames {
        let prepare = Instant::now();
        let frame = build(index);
        terrain.pipelines.prepare_atlas(&frame)?;
        let prepare_ns = prepare.elapsed().as_nanos();
        terrain.flight.begin(&frame)?;
        queries.begin(terrain.flight.command);
        terrain.record(&frame);
        queries.end(terrain.flight.command);
        terrain.flight.submit(None)?;
        terrain.flight.wait()?;
        let frame_ns = prepare.elapsed().as_nanos();
        if index >= warmup_frames {
            samples.push(TerrainFrameSample {
                prepare_ns,
                gpu_ns: queries.elapsed_ns()?,
                frame_ns,
            });
        }
    }
    Ok(samples)
}

struct TimestampQueries {
    gpu: Arc<Gpu>,
    raw: vk::QueryPool,
}

impl TimestampQueries {
    fn new(gpu: Arc<Gpu>) -> Result<Self, Error> {
        let info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(2);
        // The selected graphics queue supports timestamp queries.
        let raw = unsafe { gpu.device.create_query_pool(&info, None) }
            .map_err(vk_error("create terrain timestamp queries"))?;
        Ok(Self { gpu, raw })
    }

    fn begin(&self, command: vk::CommandBuffer) {
        // The previous frame's fence has signaled before the queries are reused.
        unsafe {
            self.gpu
                .device
                .cmd_reset_query_pool(command, self.raw, 0, 2);
            self.gpu.device.cmd_write_timestamp2(
                command,
                vk::PipelineStageFlags2::TOP_OF_PIPE,
                self.raw,
                0,
            );
        }
    }

    fn end(&self, command: vk::CommandBuffer) {
        // Complete terrain and HUD GPU commands before recording the end timestamp.
        unsafe {
            self.gpu.device.cmd_write_timestamp2(
                command,
                vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                self.raw,
                1,
            );
        }
    }

    fn elapsed_ns(&self) -> Result<u128, Error> {
        let mut timestamps = [0_u64; 2];
        // The frame fence has signaled, so both query writes are available.
        unsafe {
            self.gpu.device.get_query_pool_results(
                self.raw,
                0,
                &mut timestamps,
                vk::QueryResultFlags::TYPE_64,
            )
        }
        .map_err(vk_error("read terrain timestamp queries"))?;
        let ticks = timestamp_delta(timestamps, self.gpu.timestamp_valid_bits);
        Ok((ticks as f64 * f64::from(self.gpu.timestamp_period)).round() as u128)
    }
}

impl Drop for TimestampQueries {
    fn drop(&mut self) {
        // The offscreen renderer releases its flight before the query pool.
        unsafe { self.gpu.device.destroy_query_pool(self.raw, None) };
    }
}

fn timestamp_delta([start, end]: [u64; 2], valid_bits: u32) -> u64 {
    let mask = u64::MAX >> (64 - valid_bits);
    end.wrapping_sub(start) & mask
}

#[cfg(test)]
mod tests {
    use super::timestamp_delta;

    #[test]
    fn measures_gpu_timestamps_across_counter_wraparound() {
        assert_eq!(timestamp_delta([20, 27], 64), 7);
        assert_eq!(timestamp_delta([255, 4], 8), 5);
        assert_eq!(timestamp_delta([u64::MAX, 4], 64), 5);
    }
}
