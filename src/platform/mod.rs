use winit::window::Window;

use crate::display_hdr::DisplayHDR;

#[cfg(target_os = "macos")]
mod osx;

#[cfg(target_os = "windows")]
mod windows;

pub fn get_hdr_params(window: &Window) -> Option<DisplayHDR> {
    #[cfg(target_os = "macos")]
    {
        osx::macos_edr_headroom(window)
    }

    #[cfg(target_os = "windows")]
    {
        windows::windows_hdr_state(window)
    }
}
