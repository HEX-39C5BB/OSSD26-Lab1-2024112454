use std::ffi::{CStr, c_char};

use ash::{Device, Entry, Instance, khr, vk};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use crate::window::{Error, vk_error};

pub(crate) struct VulkanInstance {
    pub(crate) raw: Instance,
    pub(crate) _entry: Entry,
}

impl VulkanInstance {
    fn new(extensions: &[*const c_char]) -> Result<Self, Error> {
        Self::with_api_version(extensions, vk::API_VERSION_1_0)
    }

    pub(crate) fn with_api_version(
        extensions: &[*const c_char],
        version: u32,
    ) -> Result<Self, Error> {
        let app = vk::ApplicationInfo::default()
            .application_name(c"RustCraft")
            .api_version(version);
        let info = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(extensions);
        // SAFETY: Names come from ash-window or the empty list; all inputs outlive this call.
        unsafe {
            let entry = Entry::load()?;
            let raw = entry
                .create_instance(&info, None)
                .map_err(vk_error("create Vulkan instance"))?;
            Ok(Self { raw, _entry: entry })
        }
    }

    fn create_device(
        &self,
        physical: vk::PhysicalDevice,
        family: u32,
        extensions: &[*const c_char],
    ) -> Result<Device, Error> {
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&[1.0])];
        let info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_extension_names(extensions);
        // SAFETY: Callers select an available graphics queue in this instance and check extensions.
        unsafe {
            let device = self
                .raw
                .create_device(physical, &info, None)
                .map_err(vk_error("create device"))?;
            let props = self.raw.get_physical_device_properties(physical);
            let name = CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy();
            eprintln!("event=vulkan_device_ready name={name:?} queue_family={family}");
            Ok(device)
        }
    }
}

pub fn probe() -> Result<(), Error> {
    let instance = VulkanInstance::new(&[])?;
    // SAFETY: The queried devices and created logical device belong to this live instance.
    unsafe {
        let devices = instance
            .raw
            .enumerate_physical_devices()
            .map_err(vk_error("enumerate GPUs"))?;
        for physical in devices {
            let families = instance
                .raw
                .get_physical_device_queue_family_properties(physical);
            if let Some(family) = families.iter().position(|properties| {
                properties.queue_count > 0
                    && properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
            }) {
                let device = instance.create_device(physical, family as u32, &[])?;
                device.destroy_device(None);
                return Ok(());
            }
        }
    }
    Err("no Vulkan device exposes a graphics queue".into())
}

impl Drop for VulkanInstance {
    fn drop(&mut self) {
        // SAFETY: The surface and device are released before this instance.
        unsafe { self.raw.destroy_instance(None) };
    }
}

struct Surface {
    instance: VulkanInstance,
    loader: khr::surface::Instance,
    raw: vk::SurfaceKHR,
}

impl Surface {
    fn new(window: &Window) -> Result<Self, Error> {
        let display = window.display_handle()?.as_raw();
        let handle = window.window_handle()?.as_raw();
        let extensions = ash_window::enumerate_required_extensions(display)
            .map_err(vk_error("enumerate window extensions"))?;
        let instance = VulkanInstance::new(extensions)?;
        let loader = khr::surface::Instance::new(&instance._entry, &instance.raw);
        // SAFETY: The loader and window are kept alive until Vulkan resources are released.
        unsafe {
            let raw =
                ash_window::create_surface(&instance._entry, &instance.raw, display, handle, None)
                    .map_err(vk_error("create window surface"))?;
            Ok(Self {
                instance,
                loader,
                raw,
            })
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: Device and swapchain teardown are complete, and the instance is still alive.
        unsafe { self.loader.destroy_surface(self.raw, None) };
    }
}

pub(crate) struct Context {
    surface: Surface,
    physical: vk::PhysicalDevice,
    pub(crate) device: Device,
    pub(crate) family: u32,
    pub(crate) queue: vk::Queue,
}

impl Context {
    pub(crate) fn new(window: &Window) -> Result<Self, Error> {
        let surface = Surface::new(window)?;
        let instance = &surface.instance.raw;
        // SAFETY: Device and queue queries use live instance and surface handles.
        unsafe {
            let devices = instance
                .enumerate_physical_devices()
                .map_err(vk_error("enumerate GPUs"))?;
            for physical in devices {
                let extensions = instance
                    .enumerate_device_extension_properties(physical)
                    .map_err(vk_error("enumerate device extensions"))?;
                if !extensions.iter().any(|extension| {
                    extension.extension_name_as_c_str() == Ok(khr::swapchain::NAME)
                }) {
                    continue;
                }
                let families = instance.get_physical_device_queue_family_properties(physical);
                for (index, properties) in families.iter().enumerate() {
                    let family = index as u32;
                    if properties.queue_count == 0
                        || !properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                        || !surface
                            .loader
                            .get_physical_device_surface_support(physical, family, surface.raw)
                            .map_err(vk_error("query surface support"))?
                    {
                        continue;
                    }
                    let capacities = surface
                        .loader
                        .get_physical_device_surface_capabilities(physical, surface.raw)
                        .map_err(vk_error("query surface capabilities"))?;
                    if !capacities
                        .supported_usage_flags
                        .contains(vk::ImageUsageFlags::TRANSFER_DST)
                    {
                        continue;
                    }
                    let extensions = [khr::swapchain::NAME.as_ptr()];
                    let device = surface
                        .instance
                        .create_device(physical, family, &extensions)?;
                    let queue = device.get_device_queue(family, 0);
                    return Ok(Self {
                        surface,
                        physical,
                        device,
                        family,
                        queue,
                    });
                }
            }
            Err("no Vulkan device supports a graphics/present queue and transfer-destination swapchain images".into())
        }
    }

    pub(crate) fn swapchain_loader(&self) -> khr::swapchain::Device {
        khr::swapchain::Device::new(&self.surface.instance.raw, &self.device)
    }

    pub(crate) fn surface(&self) -> vk::SurfaceKHR {
        self.surface.raw
    }

    pub(crate) fn capabilities(&self) -> Result<vk::SurfaceCapabilitiesKHR, Error> {
        // SAFETY: The surface and selected physical device belong to the live instance.
        unsafe {
            self.surface
                .loader
                .get_physical_device_surface_capabilities(self.physical, self.surface.raw)
        }
        .map_err(vk_error("query surface capabilities"))
    }

    pub(crate) fn formats(&self) -> Result<Vec<vk::SurfaceFormatKHR>, Error> {
        // SAFETY: The surface and selected physical device belong to the live instance.
        unsafe {
            self.surface
                .loader
                .get_physical_device_surface_formats(self.physical, self.surface.raw)
        }
        .map_err(vk_error("query surface formats"))
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: Context is single-threaded and swapchain resources have been released.
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_device(None);
        }
    }
}
