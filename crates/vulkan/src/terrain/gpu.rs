use std::ffi::CStr;
use std::sync::Arc;

use ash::{Device, khr, vk};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use crate::context::VulkanInstance;
use crate::window::{Error, vk_error};

pub(super) struct Surface {
    pub loader: khr::surface::Instance,
    pub raw: vk::SurfaceKHR,
}

impl Drop for Surface {
    fn drop(&mut self) {
        // The swapchain is gone and the instance is still live.
        unsafe { self.loader.destroy_surface(self.raw, None) };
    }
}

pub(super) struct Gpu {
    pub info: Arc<super::GraphicsInfo>,
    pub device: Device,
    pub queue: vk::Queue,
    pub family: u32,
    pub depth_format: vk::Format,
    pub surface: Option<Surface>,
    pub physical: vk::PhysicalDevice,
    pub memory: vk::PhysicalDeviceMemoryProperties,
    pub instance: VulkanInstance,
    pub timestamp_valid_bits: u32,
    pub timestamp_period: f32,
}

impl Gpu {
    pub fn new(window: Option<&Window>) -> Result<Arc<Self>, Error> {
        Self::select(window, |_, _| true)
    }

    pub fn select(
        window: Option<&Window>,
        accepts: impl Fn(&vk::PhysicalDeviceProperties, &vk::QueueFamilyProperties) -> bool,
    ) -> Result<Arc<Self>, Error> {
        let display = window.map(|window| window.display_handle()).transpose()?;
        let extensions = display
            .as_ref()
            .map_or(Ok(&[][..]), |display| {
                ash_window::enumerate_required_extensions(display.as_raw())
            })
            .map_err(vk_error("query Vulkan window extensions"))?;
        let instance = VulkanInstance::with_api_version(extensions, vk::API_VERSION_1_3)?;

        // Window handles outlive the surface. The instance outlives each resource it creates.
        unsafe {
            let surface = if let (Some(window), Some(display)) = (window, display) {
                let raw = ash_window::create_surface(
                    &instance._entry,
                    &instance.raw,
                    display.as_raw(),
                    window.window_handle()?.as_raw(),
                    None,
                )
                .map_err(vk_error("create Vulkan surface"))?;
                Some(Surface {
                    loader: khr::surface::Instance::new(&instance._entry, &instance.raw),
                    raw,
                })
            } else {
                None
            };
            for physical in instance
                .raw
                .enumerate_physical_devices()
                .map_err(vk_error("enumerate Vulkan 1.3 devices"))?
            {
                let props = instance.raw.get_physical_device_properties(physical);
                if props.api_version < vk::API_VERSION_1_3 {
                    continue;
                }
                let mut features = vk::PhysicalDeviceVulkan13Features::default();
                let mut query = vk::PhysicalDeviceFeatures2::default().push_next(&mut features);
                instance
                    .raw
                    .get_physical_device_features2(physical, &mut query);
                if features.dynamic_rendering == 0 || features.synchronization2 == 0 {
                    continue;
                }
                let Some(depth_format) = [vk::Format::D32_SFLOAT, vk::Format::D16_UNORM]
                    .into_iter()
                    .find(|&format| {
                        instance
                            .raw
                            .get_physical_device_format_properties(physical, format)
                            .optimal_tiling_features
                            .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
                    })
                else {
                    continue;
                };
                if surface.is_some()
                    && !instance
                        .raw
                        .enumerate_device_extension_properties(physical)
                        .map_err(vk_error("list Vulkan device extensions"))?
                        .iter()
                        .any(|extension| {
                            extension.extension_name_as_c_str() == Ok(khr::swapchain::NAME)
                        })
                {
                    continue;
                }
                let families = instance
                    .raw
                    .get_physical_device_queue_family_properties(physical);
                for (family, properties) in families.iter().enumerate() {
                    if properties.queue_count == 0
                        || !properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                        || !accepts(&props, properties)
                    {
                        continue;
                    }
                    let family = family as u32;
                    if let Some(surface) = &surface
                        && !surface
                            .loader
                            .get_physical_device_surface_support(physical, family, surface.raw)
                            .map_err(vk_error("query Vulkan presentation support"))?
                    {
                        continue;
                    }
                    let queues = [vk::DeviceQueueCreateInfo::default()
                        .queue_family_index(family)
                        .queue_priorities(&[1.0])];
                    let swapchain = [khr::swapchain::NAME.as_ptr()];
                    let extensions = if surface.is_some() {
                        &swapchain[..]
                    } else {
                        &[]
                    };
                    let mut features = vk::PhysicalDeviceVulkan13Features::default()
                        .dynamic_rendering(true)
                        .synchronization2(true);
                    let info = vk::DeviceCreateInfo::default()
                        .queue_create_infos(&queues)
                        .enabled_extension_names(extensions)
                        .push_next(&mut features);
                    let device = instance
                        .raw
                        .create_device(physical, &info, None)
                        .map_err(vk_error("create Vulkan 1.3 device"))?;
                    let queue = device.get_device_queue(family, 0);
                    let memory = instance.raw.get_physical_device_memory_properties(physical);
                    let name = CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy();
                    let mut driver = vk::PhysicalDeviceDriverProperties::default();
                    let mut driver_properties =
                        vk::PhysicalDeviceProperties2::default().push_next(&mut driver);
                    instance
                        .raw
                        .get_physical_device_properties2(physical, &mut driver_properties);
                    let driver_name = driver
                        .driver_name_as_c_str()
                        .unwrap_or(c"unknown")
                        .to_string_lossy();
                    let driver_info = driver
                        .driver_info_as_c_str()
                        .unwrap_or(c"unknown")
                        .to_string_lossy();
                    let api = props.api_version;
                    eprintln!(
                        "event=vulkan_terrain_device device_type={:?} name={name:?} api={}.{}.{} driver={driver_name:?} info={driver_info:?} vendor_id=0x{:04x}",
                        props.device_type,
                        vk::api_version_major(api),
                        vk::api_version_minor(api),
                        vk::api_version_patch(api),
                        props.vendor_id,
                    );
                    return Ok(Arc::new(Self {
                        info: Arc::new(super::GraphicsInfo {
                            device: name.into_owned(),
                            driver: driver_name.into_owned(),
                            driver_info: driver_info.into_owned(),
                            vulkan_version: [
                                vk::api_version_major(api),
                                vk::api_version_minor(api),
                                vk::api_version_patch(api),
                            ],
                            queue_family: family,
                        }),
                        device,
                        queue,
                        family,
                        depth_format,
                        surface,
                        physical,
                        memory,
                        instance,
                        timestamp_valid_bits: properties.timestamp_valid_bits,
                        timestamp_period: props.limits.timestamp_period,
                    }));
                }
            }
            Err("no Vulkan 1.3 graphics device exposes dynamic rendering and Synchronization2 for this target".into())
        }
    }

    pub fn allocate(
        &self,
        requirements: vk::MemoryRequirements,
        flags: vk::MemoryPropertyFlags,
    ) -> Result<vk::DeviceMemory, Error> {
        let kind = (0..self.memory.memory_type_count)
            .find(|&i| {
                requirements.memory_type_bits & (1 << i) != 0
                    && self.memory.memory_types[i as usize]
                        .property_flags
                        .contains(flags)
            })
            .ok_or("no compatible Vulkan memory type")?;
        let info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(kind);
        // Memory type and size match the resource's requirements.
        unsafe { self.device.allocate_memory(&info, None) }
            .map_err(vk_error("allocate Vulkan memory"))
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        // Resources retain the device, and the application submits on one thread.
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_device(None);
        }
    }
}

pub(super) struct Buffer {
    gpu: Arc<Gpu>,
    pub raw: vk::Buffer,
    memory: vk::DeviceMemory,
    pub size: usize,
}

impl Buffer {
    pub fn new(gpu: Arc<Gpu>, size: usize, usage: vk::BufferUsageFlags) -> Result<Self, Error> {
        let mut buffer = Self {
            gpu,
            size,
            raw: vk::Buffer::null(),
            memory: vk::DeviceMemory::null(),
        };
        let info = vk::BufferCreateInfo::default()
            .size(size as u64)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        // The returned memory matches the buffer and is CPU-visible and coherent.
        unsafe {
            buffer.raw = buffer
                .gpu
                .device
                .create_buffer(&info, None)
                .map_err(vk_error("create terrain buffer"))?;
            let requirements = buffer.gpu.device.get_buffer_memory_requirements(buffer.raw);
            buffer.memory = buffer.gpu.allocate(
                requirements,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            buffer
                .gpu
                .device
                .bind_buffer_memory(buffer.raw, buffer.memory, 0)
                .map_err(vk_error("bind terrain buffer"))?;
        }
        Ok(buffer)
    }

    pub fn upload(&self, terrain: &[u8], hud: &[u8]) -> Result<(), Error> {
        let length = terrain.len() + hud.len();
        assert!(length > 0 && length <= self.size);
        // The frame fence was signaled before writing; the buffer is mapped only here.
        unsafe {
            let dst = self
                .gpu
                .device
                .map_memory(self.memory, 0, length as u64, vk::MemoryMapFlags::empty())
                .map_err(vk_error("map vertex buffer"))?
                .cast::<u8>();
            std::ptr::copy_nonoverlapping(terrain.as_ptr(), dst, terrain.len());
            std::ptr::copy_nonoverlapping(hud.as_ptr(), dst.add(terrain.len()), hud.len());
            self.gpu.device.unmap_memory(self.memory);
        }
        Ok(())
    }

    pub fn read(&self) -> Result<Vec<u8>, Error> {
        // The transfer-to-host barrier and fence make the coherent buffer visible to the CPU.
        unsafe {
            let ptr = self
                .gpu
                .device
                .map_memory(
                    self.memory,
                    0,
                    self.size as u64,
                    vk::MemoryMapFlags::empty(),
                )
                .map_err(vk_error("map readback buffer"))?
                .cast::<u8>();
            let result = std::slice::from_raw_parts(ptr, self.size).to_vec();
            self.gpu.device.unmap_memory(self.memory);
            Ok(result)
        }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            self.gpu.device.destroy_buffer(self.raw, None);
            self.gpu.device.free_memory(self.memory, None);
        }
    }
}

pub(super) struct Image {
    gpu: Arc<Gpu>,
    pub raw: vk::Image,
    pub view: vk::ImageView,
    memory: vk::DeviceMemory,
}

impl Image {
    pub fn new(
        gpu: Arc<Gpu>,
        extent: vk::Extent2D,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        aspect: vk::ImageAspectFlags,
    ) -> Result<Self, Error> {
        let mut image = Self {
            gpu,
            raw: vk::Image::null(),
            view: vk::ImageView::null(),
            memory: vk::DeviceMemory::null(),
        };
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .usage(usage);
        unsafe {
            image.raw = image
                .gpu
                .device
                .create_image(&info, None)
                .map_err(vk_error("create render image"))?;
            let requirements = image.gpu.device.get_image_memory_requirements(image.raw);
            image.memory = image
                .gpu
                .allocate(requirements, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
            image
                .gpu
                .device
                .bind_image_memory(image.raw, image.memory, 0)
                .map_err(vk_error("bind render image"))?;
        }
        image.view = view(&image.gpu, image.raw, format, aspect)?;
        Ok(image)
    }

    pub fn depth(gpu: Arc<Gpu>, extent: vk::Extent2D) -> Result<Self, Error> {
        let format = gpu.depth_format;
        Self::new(
            gpu,
            extent,
            format,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::ImageAspectFlags::DEPTH,
        )
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            self.gpu.device.destroy_image_view(self.view, None);
            self.gpu.device.destroy_image(self.raw, None);
            self.gpu.device.free_memory(self.memory, None);
        }
    }
}

pub(super) fn view(
    gpu: &Gpu,
    image: vk::Image,
    format: vk::Format,
    aspect: vk::ImageAspectFlags,
) -> Result<vk::ImageView, Error> {
    let info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .subresource_range(range(aspect));
    unsafe { gpu.device.create_image_view(&info, None) }
        .map_err(vk_error("create terrain image view"))
}

pub(super) fn range(aspect: vk::ImageAspectFlags) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(aspect)
        .level_count(1)
        .layer_count(1)
}
