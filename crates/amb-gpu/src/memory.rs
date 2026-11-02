// SPDX-License-Identifier: GPL-3.0-or-later
//! How much GPU memory a render may still use, as the driver reports it:
//! wgpu has no call for this, so each graphics API is asked in its own way.
//!
//! - Vulkan (Linux, Windows with Vulkan): the `VK_EXT_memory_budget`
//!   extension, summed over the device-local heaps.
//! - Direct3D 12 (Windows): DXGI's video memory info for the same adapter.
//! - Metal (macOS): the device's recommended working set minus what this
//!   process holds.
//!
//! `None` where the API or driver can't say; the caller then uses a fixed
//! budget.

/// Bytes this process may still allocate on `adapter`'s GPU, if known.
pub fn available(adapter: &wgpu::Adapter) -> Option<u64> {
    let info = adapter.get_info();
    match info.backend {
        #[cfg(not(any(target_vendor = "apple", target_arch = "wasm32")))]
        wgpu::Backend::Vulkan => vulkan(adapter),
        #[cfg(windows)]
        wgpu::Backend::Dx12 => dxgi(info.vendor, info.device),
        #[cfg(target_vendor = "apple")]
        wgpu::Backend::Metal => metal(),
        _ => None,
    }
}

#[cfg(not(any(target_vendor = "apple", target_arch = "wasm32")))]
fn vulkan(adapter: &wgpu::Adapter) -> Option<u64> {
    use ash::vk;
    // SAFETY: the handles are only read, while `adapter` is alive, and the
    // budget struct is chained as the Vulkan spec describes.
    unsafe {
        let hal = adapter.as_hal::<wgpu::hal::api::Vulkan>()?;
        let instance = hal.shared_instance().raw_instance();
        let device = hal.raw_physical_device();
        let extensions = instance
            .enumerate_device_extension_properties(device)
            .ok()?;
        let has_budget = extensions
            .iter()
            .any(|e| e.extension_name_as_c_str() == Ok(ash::ext::memory_budget::NAME));
        if !has_budget {
            return None;
        }
        let mut budget = vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
        let mut props = vk::PhysicalDeviceMemoryProperties2::default().push_next(&mut budget);
        instance.get_physical_device_memory_properties2(device, &mut props);
        let heaps = props.memory_properties.memory_heaps;
        let count = props.memory_properties.memory_heap_count as usize;
        let free: u64 = (0..count.min(heaps.len()))
            .filter(|&i| heaps[i].flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
            .map(|i| budget.heap_budget[i].saturating_sub(budget.heap_usage[i]))
            .sum();
        (free > 0).then_some(free)
    }
}

#[cfg(windows)]
fn dxgi(vendor: u32, device: u32) -> Option<u64> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_QUERY_VIDEO_MEMORY_INFO,
        IDXGIAdapter1, IDXGIAdapter3, IDXGIFactory1,
    };
    use windows::core::Interface;
    // SAFETY: plain COM calls on objects this function owns.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        for i in 0.. {
            let adapter: IDXGIAdapter1 = factory.EnumAdapters1(i).ok()?;
            let desc = adapter.GetDesc1().ok()?;
            if desc.VendorId != vendor || desc.DeviceId != device {
                continue;
            }
            let adapter: IDXGIAdapter3 = adapter.cast().ok()?;
            let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
            adapter
                .QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info)
                .ok()?;
            return Some(info.Budget.saturating_sub(info.CurrentUsage));
        }
        None
    }
}

#[cfg(target_vendor = "apple")]
fn metal() -> Option<u64> {
    let device = metal::Device::system_default()?;
    let budget = device.recommended_max_working_set_size();
    let used = device.current_allocated_size();
    Some(budget.saturating_sub(used))
}

#[cfg(test)]
mod tests {
    use crate::device::tests::gpu;

    #[test]
    fn the_driver_says_how_much_memory_is_free() {
        let Some(gpu) = gpu() else { return };
        let free = super::available(&gpu.adapter);
        eprintln!("{}: free {:?} MB", gpu.name(), free.map(|b| b >> 20));
        // Every backend we ask on these platforms answers when its driver can.
        if let Some(b) = free {
            assert!(b > 16 << 20, "{b}");
        }
        assert!(gpu.memory_budget() > 0);
    }
}
