#[cfg(target_os = "macos")]
use objc2::{msg_send, runtime::AnyObject};
#[cfg(target_os = "macos")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(target_os = "macos")]
use winit::window::Window;

#[cfg(target_os = "macos")]
pub fn macos_edr_headroom(window: &Window) -> Option<(f32, f32)> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };

    let ns_view = appkit.ns_view.as_ptr().cast::<AnyObject>();
    if ns_view.is_null() {
        return None;
    }

    let ns_window: *mut AnyObject = unsafe { msg_send![ns_view, window] };
    if ns_window.is_null() {
        return None;
    }

    let ns_screen: *mut AnyObject = unsafe { msg_send![ns_window, screen] };
    if ns_screen.is_null() {
        return None;
    }

    let current: f64 =
        unsafe { msg_send![ns_screen, maximumExtendedDynamicRangeColorComponentValue] };
    let potential: f64 = unsafe {
        msg_send![
            ns_screen,
            maximumPotentialExtendedDynamicRangeColorComponentValue
        ]
    };
    Some((current as f32, potential as f32))
}
