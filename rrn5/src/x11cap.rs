// Fenster-Capture über X11 (Taste "i" in main):
// Zeiger greifen, auf Klick warten, das angeklickte Fenster in output.ppm speichern.
// Entspricht captureWindow() bzw. dem SDLK_i-Handler in rrn5.cpp.
use std::ffi::CString;
use std::mem::MaybeUninit;
use std::os::raw::{c_char, c_int};

use x11::xlib::{
    self, ButtonPress, CurrentTime, GrabModeAsync, GrabSuccess, XAllPlanes, ZPixmap,
};

static BUTTON_PRESS_MASK: c_int = 0x0000_0004;

fn err(msg: &str) -> String {
    eprintln!("{msg}");
    "capture failed".to_string()
}

/// Grabt den Poiter, wartet auf einen Klick und capturet das getroffene Fenster
/// als PPM nach <cwd>/output.ppm. Liefert () nur bei Erfolg inkl. Klick.
pub unsafe fn capture_clicked_window() -> Result<(), String> {
    let dpy = xlib::XOpenDisplay(std::ptr::null());
    if dpy.is_null() {
        return Err(err("Unable to open display"));
    }
    let root = xlib::XDefaultRootWindow(dpy);
    let res = xlib::XGrabPointer(
        dpy,
        root,
        0,
        BUTTON_PRESS_MASK as u32,
        GrabModeAsync,
        GrabModeAsync,
        0,
        0,
        CurrentTime.into(),
    );
    if res != GrabSuccess {
        xlib::XCloseDisplay(dpy);
        return Err(err("Unable to grab pointer"));
    }

    let mut event = MaybeUninit::zeroed();
    xlib::XNextEvent(dpy, event.as_mut_ptr());
    let event = event.assume_init();
    xlib::XUngrabPointer(dpy, CurrentTime.into());

    if event.get_type() != ButtonPress {
        xlib::XCloseDisplay(dpy);
        return Err(err("expected ButtonPress"));
    }

    let mut window = event.button.subwindow;
    if window == 0 {
        window = root;
    }

    let result = capture_window(dpy, window);
    xlib::XCloseDisplay(dpy);
    result
}

unsafe fn capture_window(dpy: *mut xlib::Display, window: xlib::Window) -> Result<(), String> {
    let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
    xlib::XGetWindowAttributes(dpy, window, &mut attrs);

    let img = xlib::XGetImage(
        dpy,
        window,
        0,
        0,
        attrs.width as u32,
        attrs.height as u32,
        XAllPlanes(),
        ZPixmap,
    );
    if img.is_null() {
        return Err(err("XGetImage failed"));
    }

    let red_mask = (*img).red_mask;
    let green_mask = (*img).green_mask;
    let blue_mask = (*img).blue_mask;

    let mut ppm: Vec<u8> = Vec::with_capacity(((*img).width * (*img).height * 3 + 32) as usize);
    let header = format!("P6\n{} {}\n255\n", (*img).width, (*img).height);
    ppm.extend_from_slice(header.as_bytes());
    for y in 0..(*img).height {
        for x in 0..(*img).width {
            let pixel = xlib::XGetPixel(img, x, y);
            let b = (pixel & blue_mask) as u8;
            let g = ((pixel & green_mask) >> 8) as u8;
            let r = ((pixel & red_mask) >> 16) as u8;
            ppm.push(r);
            ppm.push(g);
            ppm.push(b);
        }
    }
    xlib::XDestroyImage(img);

    std::fs::write("output.ppm", ppm).map_err(|e| err(&format!("output.ppm: {e}")))
}

/// Prüft, ob ein C-String (vom C-String-Helfer) null ist. (unused guard)
pub fn _c_string(s: &str) -> CString {
    CString::new(s).unwrap()
}