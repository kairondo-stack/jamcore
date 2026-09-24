use sdl2::sys as s;
use std::ffi::CString;

fn main() {
    unsafe {
        s::SDL_Init(s::SDL_INIT_VIDEO);
        let title = CString::new("probe").unwrap();
        let w = s::SDL_CreateWindow(
            title.as_ptr(), 0, 0, 200, 200, 0,
        );
        let mut ev: s::SDL_Event = std::mem::zeroed();
        for i in 0..200 {
            if s::SDL_PollEvent(&mut ev) != 0 {
                println!("iter {} raw type=0x{:x}", i, ev.type_);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if !w.is_null() {
            s::SDL_DestroyWindow(w);
        }
        s::SDL_Quit();
    }
}
