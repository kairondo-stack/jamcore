/// Test-Hook: E2E_KEYS="Right,Right,S" beim Start in die SDL-Queue legen.
fn inject_test_keys() {
    let Ok(spec) = std::env::var("E2E_KEYS") else {
        return;
    };
    std::thread::spawn(move || push_test_keys(&spec));
}

fn push_test_keys(spec: &str) {
    use sdl2::keyboard::{Keycode as K, Scancode as S};
    for name in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(ms) = name.strip_prefix("wait:") {
            std::thread::sleep(std::time::Duration::from_millis(ms.parse().unwrap_or(0)));
            continue;
        }
        if name.eq_ignore_ascii_case("quit") {
            unsafe {
                let mut raw = std::mem::zeroed::<sdl2::sys::SDL_Event>();
                raw.type_ = sdl2::sys::SDL_EventType::SDL_QUIT as u32;
                sdl2::sys::SDL_PushEvent(&mut raw);
            }
            continue;
        }
        let (k, _s) = match name.to_ascii_lowercase().as_str() {
            "tab" => (K::Tab, S::Tab),
            "right" => (K::Right, S::Right),
            "left" => (K::Left, S::Left),
            "up" => (K::Up, S::Up),
            "down" => (K::Down, S::Down),
            "return" => (K::Return, S::Return),
            "0" => (K::Num0, S::Num0),
            "1" => (K::Num1, S::Num1),
            "2" => (K::Num2, S::Num2),
            "3" => (K::Num3, S::Num3),
            "4" => (K::Num4, S::Num4),
            "5" => (K::Num5, S::Num5),
            "6" => (K::Num6, S::Num6),
            "7" => (K::Num7, S::Num7),
            "8" => (K::Num8, S::Num8),
            "9" => (K::Num9, S::Num9),
            "a" => (K::A, S::A),
            "b" => (K::B, S::B),
            "c" => (K::C, S::C),
            "d" => (K::D, S::D),
            "e" => (K::E, S::E),
            "f" => (K::F, S::F),
            "g" => (K::G, S::G),
            "h" => (K::H, S::H),
            "i" => (K::I, S::I),
            "j" => (K::J, S::J),
            "k" => (K::K, S::K),
            "l" => (K::L, S::L),
            "m" => (K::M, S::M),
            "n" => (K::N, S::N),
            "o" => (K::O, S::O),
            "p" => (K::P, S::P),
            "q" => (K::Q, S::Q),
            "r" => (K::R, S::R),
            "s" => (K::S, S::S),
            "t" => (K::T, S::T),
            "u" => (K::U, S::U),
            "v" => (K::V, S::V),
            "w" => (K::W, S::W),
            "x" => (K::X, S::X),
            "y" => (K::Y, S::Y),
            "z" => (K::Z, S::Z),
            "f5" => (K::F5, S::F5),
            "f8" => (K::F8, S::F8),
            "f9" => (K::F9, S::F9),
            _ => continue,
        };
        unsafe {
            let mut raw = std::mem::zeroed::<sdl2::sys::SDL_Event>();
            raw.key.type_ = sdl2::sys::SDL_EventType::SDL_KEYDOWN as u32;
            raw.key.state = 1;
            raw.key.keysym.sym = k.into_i32();
            raw.key.keysym.scancode = sdl2::sys::SDL_GetScancodeFromKey(k.into_i32());
            let rc = sdl2::sys::SDL_PushEvent(&mut raw);
            eprintln!("E2E_KEYS: pushed {name} -> {rc}");
        }
    }
}

fn poll_event_compat() -> Option<sdl2::event::Event> {
    unsafe {
        loop {
            let mut raw = std::mem::MaybeUninit::uninit();
            if sdl2::sys::SDL_PollEvent(raw.as_mut_ptr()) != 1 {
                return None;
            }
            let ev = raw.assume_init();
            let t = ev.type_;
            if t == sdl2::sys::SDL_EventType::SDL_QUIT as u32
                || t == sdl2::sys::SDL_EventType::SDL_KEYDOWN as u32
                || t == sdl2::sys::SDL_EventType::SDL_KEYUP as u32
            {
                return Some(sdl2::event::Event::from_ll(ev));
            }
        }
    }
}

#[derive(Clone, Copy)]
#[repr(C, align(4))]
struct SharedData2 {
    width: i32,
    height: i32,
    pitch: i32,
    format: u32,
    rmask: u32,
    gmask: u32,
    bmask: u32,
    amask: u32,
    bpp: i32,
}

const DATA_OFFSET: usize = std::mem::size_of::<SharedData2>();
/// Festes Mapping (Boost hält bis zum Ende; truncate pro Bild nicht nötig).
const MAPPING_SIZE: usize = DATA_OFFSET + 4096 * 4096 * 4;

fn bytes_le(h: &SharedData2) -> [u8; DATA_OFFSET] {
    let mut out = [0u8; DATA_OFFSET];
    let mut o = 0;
    for v in [
        h.width.to_le_bytes(),
        h.height.to_le_bytes(),
        h.pitch.to_le_bytes(),
        h.format.to_le_bytes(),
        h.rmask.to_le_bytes(),
        h.gmask.to_le_bytes(),
        h.bmask.to_le_bytes(),
        h.amask.to_le_bytes(),
        h.bpp.to_le_bytes(),
    ] {
        out[o..o + 4].copy_from_slice(&v);
        o += 4;
    }
    out
}

/// Schreibt den Boost-kompatiblen SharedData2-Header + RGBA-Pixeldaten in MyImageShm.
fn write_shm(sh: &mut shared_memory::Shmem, width: i32, height: i32, pixels: &[u8]) -> Result<(), String> {
    let pitch = width * 4;
    let header = SharedData2 {
        width,
        height,
        pitch,
        format: 0x16762004, // SDL_PIXELFORMAT_RGBA32 (wie IMG_Load auf LE-Systemen)
        rmask: 0x000000ff,
        gmask: 0x0000ff00,
        bmask: 0x00ff0000,
        amask: 0xff000000,
        bpp: 32,
    };
    if pixels.len() > sh.len() - DATA_OFFSET {
        return Err(format!(
            "Bild zu groß: {} > Mapping-Kapazität {}",
            pixels.len(),
            sh.len() - DATA_OFFSET
        ));
    }
    unsafe {
        let s = sh.as_slice_mut();
        let (hd, px) = s.split_at_mut(DATA_OFFSET);
        hd.copy_from_slice(&bytes_le(&header));
        px[..pixels.len()].copy_from_slice(pixels);
    }
    Ok(())
}

/// Öffnet oder erzeugt das MapImageMapping; Besitzer räumt beim Beenden auf.
fn create_or_open_shm() -> Result<shared_memory::Shmem, String> {
    match shared_memory::ShmemConf::new()
        .os_id("MyImageShm")
        .size(MAPPING_SIZE)
        .create()
    {
        Ok(sh) => Ok(sh),
        Err(shared_memory::ShmemError::MappingIdExists) => {
            shared_memory::ShmemConf::new()
                .os_id("MyImageShm")
                .open()
                .map_err(|e| format!("open MyImageShm: {e:?}"))
        }
        Err(e) => Err(format!("shared memory: {e:?}")),
    }
}

fn draw(
    canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
    texture: &Option<sdl2::render::Texture<'_>>,
) -> Result<(), String> {
    canvas.clear();
    if let Some(ref t) = texture {
        canvas.copy(t, None, None).map_err(|e| e.to_string())?;
    }
    canvas.present();
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = std::env::args().nth(1) else {
        eprintln!("Verwendung: imgsh <verzeichnis>");
        std::process::exit(1);
    };

    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let p = entry?.path();
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ["png", "jpg", "jpeg", "bmp"].contains(&ext.as_str()) {
            files.push(p);
        }
    }
    files.sort();
    if files.is_empty() {
        eprintln!("Keine Bilder im Verzeichnis gefunden.");
        std::process::exit(1);
    }

    let sdl = sdl2::init().map_err(|e| e.to_string())?;
    let video = sdl.video().map_err(|e| e.to_string())?;
    let window = video
        .window("Image Viewer", 300, 300)
        .allow_highdpi()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window
        .into_canvas()
        .accelerated()
        .build()
        .map_err(|e| e.to_string())?;
    let mut events = sdl.event_pump().map_err(|e| e.to_string())?;
    let _ = events;
    inject_test_keys();
    let texture_creator = canvas.texture_creator();
    let mut shmem = create_or_open_shm().map_err(|e| e.to_string())?;

    let mut index = 0usize;
    let mut texture: Option<sdl2::render::Texture<'_>> = None;

    // Erstes Bild laden
    if let Err(e) = load_image(&mut files, index, &texture_creator, &mut shmem, &mut texture) {
        eprintln!("Bild laden fehlgeschlagen: {e}");
    }
    let _ = draw(&mut canvas, &texture);

    'main: loop {
        if let Some(ev) = poll_event_compat() {
            match ev {
                sdl2::event::Event::Quit { .. } => break 'main,
                sdl2::event::Event::KeyDown { keycode: Some(keycode), .. } => match keycode {
                    sdl2::keyboard::Keycode::S => {
                        // Snapshot: Framebuffer als minimale TGA-Datei
                        if let (Ok(px), Ok((fw, fh))) = (
                            canvas.read_pixels(None, sdl2::pixels::PixelFormatEnum::RGBA32),
                            canvas.output_size(),
                        ) {
                            let mut bmp = Vec::new();
                            bmp.extend_from_slice(&[0, 0, 2, 0, 0, 0, 0, 0]);
                            bmp.extend_from_slice(&(fw as u16).to_le_bytes());
                            bmp.extend_from_slice(&(fh as u16).to_le_bytes());
                            bmp.push(32);
                            bmp.push(0x28);
                            for y in (0..fh).rev() {
                                for x in 0..fw {
                                    let idx = ((y * fw + x) * 4) as usize;
                                    bmp.extend_from_slice(&[
                                        px[idx + 2],
                                        px[idx + 1],
                                        px[idx],
                                        px[idx + 3],
                                    ]);
                                }
                            }
                            let _ = std::fs::write("snapshot.tga", &bmp);
                            println!("Snapshot geschrieben: snapshot.tga");
                        }
                    }
                    sdl2::keyboard::Keycode::Right => {
                        index = (index + 1) % files.len();
                        let _ = load_image(&mut files, index, &texture_creator, &mut shmem, &mut texture);
                        let _ = draw(&mut canvas, &texture);
                    }
                    sdl2::keyboard::Keycode::Left => {
                        index = (index + files.len() - 1) % files.len();
                        let _ = load_image(&mut files, index, &texture_creator, &mut shmem, &mut texture);
                        let _ = draw(&mut canvas, &texture);
                    }
                    sdl2::keyboard::Keycode::Return => {
                        let _ = load_image(&mut files, index, &texture_creator, &mut shmem, &mut texture);
                        let _ = draw(&mut canvas, &texture);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    // Mapping-Besitzer (shmem) wird beim Verlassen gedroppt und räumt per shm_unlink auf.
    Ok(())
}

fn load_image<'a>(
    files: &mut Vec<std::path::PathBuf>,
    i: usize,
    tc: &'a sdl2::render::TextureCreator<sdl2::video::WindowContext>,
    shmem: &mut shared_memory::Shmem,
    texture: &mut Option<sdl2::render::Texture<'a>>,
) -> Result<(), String> {
    let img = image::open(&files[i]).map_err(|e| e.to_string())?.to_rgba8();
    let (w, h) = (img.width() as i32, img.height() as i32);
    let pixels = img.into_raw();

    let mut tx = tc
        .create_texture_streaming(sdl2::pixels::PixelFormatEnum::RGBA32, w as u32, h as u32)
        .map_err(|e| e.to_string())?;
    tx.with_lock(None, |dst, dstpitch| {
        for row in 0..h as usize {
            let src = &pixels[row * (w as usize) * 4..(row + 1) * (w as usize) * 4];
            dst[row * dstpitch..row * dstpitch + src.len()].copy_from_slice(src);
        }
    })
    .map_err(|e| e.to_string())?;

    write_shm(shmem, w, h, &pixels)?;
    *texture = Some(tx);
    Ok(())
}