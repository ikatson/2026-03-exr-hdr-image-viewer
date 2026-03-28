#[cfg(target_os = "windows")]
use std::mem::size_of;

#[cfg(target_os = "windows")]
use windows::{
    Win32::{
        Devices::Display::{
            DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
            DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER,
            DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SDR_WHITE_LEVEL,
            DISPLAYCONFIG_SOURCE_DEVICE_NAME, DisplayConfigGetDeviceInfo,
            GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
        },
        Foundation::{ERROR_INSUFFICIENT_BUFFER, HWND, RECT},
        Graphics::Dxgi::{
            Common::{
                DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
                DXGI_COLOR_SPACE_RGB_STUDIO_G2084_NONE_P2020,
            },
            CreateDXGIFactory1, DXGI_OUTPUT_DESC1, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput6,
        },
        UI::WindowsAndMessaging::GetWindowRect,
    },
    core::Interface,
};
#[cfg(target_os = "windows")]
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

#[cfg(target_os = "windows")]
pub fn windows_hdr_headroom(window: &Window) -> Option<(f32, f32)> {
    let output = dxgi_output_for_window(window)?;
    let sdr_white_nits = sdr_white_level_nits(&output.device_name).unwrap_or(80.0);
    let peak_nits = output.desc.MaxLuminance;
    if !peak_nits.is_finite() || peak_nits <= 0.0 {
        return None;
    }

    let potential = (peak_nits / sdr_white_nits.max(1.0)).max(1.0);
    let current = if is_hdr_colorspace(output.desc.ColorSpace) {
        potential
    } else {
        1.0
    };
    return Some((current * sdr_white_nits / 80., potential));
    // dbg!((output.desc, &sdr_white_nits, peak_nits, potential, current));
    Some((current, potential))
}

#[cfg(target_os = "windows")]
struct OutputInfo {
    device_name: String,
    desc: DXGI_OUTPUT_DESC1,
}

#[cfg(target_os = "windows")]
fn hwnd_for_window(window: &Window) -> Option<HWND> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return None;
    };

    let hwnd = HWND(win32.hwnd.get() as _);
    if hwnd.0.is_null() {
        return None;
    }

    Some(hwnd)
}

#[cfg(target_os = "windows")]
fn window_rect(window: &Window) -> Option<RECT> {
    let hwnd = hwnd_for_window(window)?;
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return None;
    }
    Some(rect)
}

#[cfg(target_os = "windows")]
fn dxgi_output_for_window(window: &Window) -> Option<OutputInfo> {
    let window_rect = window_rect(window)?;
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1().ok()? };
    let mut best_output = None;
    let mut best_intersection = -1i64;

    let mut adapter_index = 0;
    loop {
        let adapter: IDXGIAdapter1 = match unsafe { factory.EnumAdapters1(adapter_index) } {
            Ok(adapter) => adapter,
            Err(_) => break,
        };
        adapter_index += 1;

        let mut output_index = 0;
        loop {
            let output = match unsafe { adapter.EnumOutputs(output_index) } {
                Ok(output) => output,
                Err(_) => break,
            };
            output_index += 1;

            let output6: IDXGIOutput6 = match output.cast() {
                Ok(output6) => output6,
                Err(_) => continue,
            };
            let desc = match unsafe { output6.GetDesc1() } {
                Ok(desc) => desc,
                Err(_) => continue,
            };
            let intersection = intersection_area(window_rect, desc.DesktopCoordinates);
            if intersection > best_intersection {
                best_intersection = intersection;
                best_output = Some(OutputInfo {
                    device_name: wide_to_string(&desc.DeviceName),
                    desc,
                });
            }
        }
    }

    best_output
}

#[cfg(target_os = "windows")]
fn sdr_white_level_nits(device_name: &str) -> Option<f32> {
    let paths = active_display_paths()?;

    for path in &paths {
        let Some(source_name) = source_device_name(path) else {
            continue;
        };
        if source_name != device_name {
            continue;
        }

        let mut sdr_white = DISPLAYCONFIG_SDR_WHITE_LEVEL::default();
        sdr_white.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
            size: size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        };

        let status = unsafe { DisplayConfigGetDeviceInfo(&mut sdr_white.header) };
        if status == 0 {
            return Some(sdr_white.SDRWhiteLevel as f32 * 80.0 / 1000.0);
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn active_display_paths() -> Option<Vec<DISPLAYCONFIG_PATH_INFO>> {
    for _ in 0..2 {
        let mut path_count = 0;
        let mut mode_count = 0;
        let size_status = unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
        };
        if size_status.0 != 0 {
            return None;
        }

        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![Default::default(); mode_count as usize];
        let query_status = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
        };
        if query_status.0 == 0 {
            paths.truncate(path_count as usize);
            return Some(paths);
        }
        if query_status != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn source_device_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    let mut source_name = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
    source_name.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
        size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
        adapterId: path.sourceInfo.adapterId,
        id: path.sourceInfo.id,
    };

    let status = unsafe { DisplayConfigGetDeviceInfo(&mut source_name.header) };
    if status != 0 {
        return None;
    }

    Some(wide_to_string(&source_name.viewGdiDeviceName))
}

#[cfg(target_os = "windows")]
fn is_hdr_colorspace(
    color_space: windows::Win32::Graphics::Dxgi::Common::DXGI_COLOR_SPACE_TYPE,
) -> bool {
    color_space == DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020
        || color_space == DXGI_COLOR_SPACE_RGB_STUDIO_G2084_NONE_P2020
}

#[cfg(target_os = "windows")]
fn intersection_area(a: RECT, b: RECT) -> i64 {
    let left = a.left.max(b.left);
    let top = a.top.max(b.top);
    let right = a.right.min(b.right);
    let bottom = a.bottom.min(b.bottom);
    let width = (right - left).max(0) as i64;
    let height = (bottom - top).max(0) as i64;
    width * height
}

#[cfg(target_os = "windows")]
fn wide_to_string(wide: &[u16]) -> String {
    let len = wide.iter().position(|&ch| ch == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..len])
}
