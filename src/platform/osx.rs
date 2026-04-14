use std::sync::atomic::{AtomicU32, Ordering};

use core_foundation_sys::{
    base::{CFRelease, CFShow, CFTypeRef},
    dictionary::CFDictionaryRef,
};
use objc2::{msg_send, runtime::AnyObject};
use objc2_foundation::NSString;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::display_hdr::DisplayHDR;

#[link(name = "CoreDisplay", kind = "framework")]
unsafe extern "C" {
    fn CoreDisplay_DisplayCreateInfoDictionary(display: u32) -> CFDictionaryRef;
}

static LAST_PRINTED_DISPLAY_ID: AtomicU32 = AtomicU32::new(0);

pub fn macos_edr_headroom(window: &Window) -> Option<DisplayHDR> {
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

    if let Some(display_id) = screen_display_id(ns_screen) {
        print_display_info_dictionary(display_id);
    }

    // let potential: f64 =
    //     unsafe { msg_send![ns_screen, maximumExtendedDynamicRangeColorComponentValue] };
    // dbg!((current, potential));

    const SDR_WHITE_NITS: f32 = 300.;
    const NITS_TO_OUTPUT_SCALE: f32 = 1. / SDR_WHITE_NITS;

    Some(DisplayHDR {
        sdr_white_nits: SDR_WHITE_NITS,
        peak_luma_nits: (SDR_WHITE_NITS as f64 * current) as f32,
        nits_to_output_scale: NITS_TO_OUTPUT_SCALE,
    })
}

fn screen_display_id(ns_screen: *mut AnyObject) -> Option<u32> {
    let device_description: *mut AnyObject = unsafe { msg_send![ns_screen, deviceDescription] };
    if device_description.is_null() {
        return None;
    }

    let key = NSString::from_str("NSScreenNumber");
    let screen_number: *mut AnyObject =
        unsafe { msg_send![device_description, objectForKey: &*key] };
    if screen_number.is_null() {
        return None;
    }

    let display_id: u32 = unsafe { msg_send![screen_number, unsignedIntValue] };
    Some(display_id)
}

fn print_display_info_dictionary(display_id: u32) {
    if LAST_PRINTED_DISPLAY_ID.swap(display_id, Ordering::Relaxed) == display_id {
        return;
    }

    let info = unsafe { CoreDisplay_DisplayCreateInfoDictionary(display_id) };
    if info.is_null() {
        eprintln!("CoreDisplay_DisplayCreateInfoDictionary({display_id}) returned null");
        return;
    }

    println!("CoreDisplay info dictionary for display {display_id}:");
    unsafe {
        CFShow(info as CFTypeRef);
        CFRelease(info as CFTypeRef);
    }
}
