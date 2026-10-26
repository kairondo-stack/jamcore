// Shared Memory-Zugriff, Boost-kompatibel (wie rrn5.cpp):
//   - "ZeitfaktoSharedMemory": SharedData (5 floats)
//   - "MyImageShm":            SharedData2-Header + Pixeldaten
// Rrn5 liest nur (open_only); imgsh/pattern_recorder schreiben.
use shared_memory::{Shmem, ShmemConf};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SharedData {
    pub zeitfakto: f32,
    pub variable2: f32,
    pub variable3: f32,
    pub dauer: f32,
    pub bpm: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SharedData2 {
    pub width: i32,
    pub height: i32,
    pub pitch: i32,
    pub format: u32,
    pub rmask: u32,
    pub gmask: u32,
    pub bmask: u32,
    pub amask: u32,
    pub bpp: i32,
}

pub struct SharedMemoryEditor {
    _shm: Shmem,
    _shm2: Shmem,
}

/// Feste Mapping-Größe für MyImageShm (wie imgsh).
pub const MAPPING_SIZE: usize = std::mem::size_of::<SharedData2>() + 4096 * 4096 * 4;

fn read_f32_at(s: &[u8], off: usize) -> f32 {
    f32::from_le_bytes([s[off], s[off + 1], s[off + 2], s[off + 3]])
}

fn bytes_le(h: &SharedData2) -> [u8; std::mem::size_of::<SharedData2>()] {
    let mut out = [0u8; std::mem::size_of::<SharedData2>()];
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

impl SharedMemoryEditor {
    pub fn open() -> Result<Self, String> {
        // Auf der Konsole (KMSDRM) läuft kein pattern_recorder, der die
        // Zeitfakto-Segment anlegt. Dann legt rrn5 sie selbst an (1024 Bytes
        // wie der Recorder) — sonst bricht der Start mit MapOpenFailed ab.
        let shm = match ShmemConf::new().os_id("ZeitfaktoSharedMemory").open() {
            Ok(s) => s,
            Err(_) => ShmemConf::new()
                .os_id("ZeitfaktoSharedMemory")
                .size(1024)
                .create()
                .map_err(|e| format!("ZeitfaktoSharedMemory: {e:?}"))?,
        };
        let shm2 = match ShmemConf::new()
            .os_id("MyImageShm")
            .size(MAPPING_SIZE)
            .create()
        {
            Ok(s) => s,
            Err(shared_memory::ShmemError::MappingIdExists) => ShmemConf::new()
                .os_id("MyImageShm")
                .open()
                .map_err(|e| format!("MyImageShm: {e:?}"))?,
            Err(e) => return Err(format!("MyImageShm: {e:?}")),
        };
        if shm.len() < 20 || shm2.len() < std::mem::size_of::<SharedData2>() {
            return Err("Shared Memory zu klein".to_string());
        }
        Ok(SharedMemoryEditor { _shm: shm, _shm2: shm2 })
    }

    /// Schreibt SharedData2-Header + RGBA-Pixel nach MyImageShm (wie imgsh).
    pub fn write_image(&mut self, width: i32, height: i32, pixels: &[u8]) -> Result<(), String> {
        if width <= 0 || height <= 0 {
            return Err(format!("ungültige Bildgröße {width}x{height}"));
        }
        let pitch = width * 4;
        let needed = pitch as usize * height as usize;
        if pixels.len() < needed {
            return Err(format!(
                "Pixelpuffer zu klein: {} < {}",
                pixels.len(),
                needed
            ));
        }
        if needed + std::mem::size_of::<SharedData2>() > self._shm2.len() {
            return Err(format!("Bild zu groß: {needed} Bytes"));
        }
        let header = SharedData2 {
            width,
            height,
            pitch,
            format: 0x16762004,
            rmask: 0x000000ff,
            gmask: 0x0000ff00,
            bmask: 0x00ff0000,
            amask: 0xff000000,
            bpp: 32,
        };
        unsafe {
            let s = self._shm2.as_slice_mut();
            let (hd, px) = s.split_at_mut(std::mem::size_of::<SharedData2>());
            hd.copy_from_slice(&bytes_le(&header));
            px[..needed].copy_from_slice(&pixels[..needed]);
        }
        Ok(())
    }

    pub fn shared_data(&self) -> SharedData {
        let s = unsafe { self._shm.as_slice() };
        SharedData {
            zeitfakto: read_f32_at(s, 0),
            variable2: read_f32_at(s, 4),
            variable3: read_f32_at(s, 8),
            dauer: read_f32_at(s, 12),
            bpm: read_f32_at(s, 16),
        }
    }

    pub fn print_current_values(&self) {
        let d = self.shared_data();
        println!("Current values:");
        println!("1. zeitfakto: {}", d.zeitfakto);
        println!("2. variable2: {}", d.variable2);
        println!("3. variable3: {}", d.variable3);
        println!("4. dauer: {}", d.dauer);
        println!("5. bpm: {}", d.bpm);
    }

    /// SharedData2-Kopf und Pixeldaten aus MyImageShm holen.
    pub fn image_header_and_pixels(&self) -> (SharedData2, &[u8]) {
        let s = unsafe { self._shm2.as_slice() };
        let off = std::mem::size_of::<SharedData2>();
        let h = SharedData2 {
            width: i32::from_le_bytes(s[0..4].try_into().unwrap()),
            height: i32::from_le_bytes(s[4..8].try_into().unwrap()),
            pitch: i32::from_le_bytes(s[8..12].try_into().unwrap()),
            format: u32::from_le_bytes(s[12..16].try_into().unwrap()),
            rmask: u32::from_le_bytes(s[16..20].try_into().unwrap()),
            gmask: u32::from_le_bytes(s[20..24].try_into().unwrap()),
            bmask: u32::from_le_bytes(s[24..28].try_into().unwrap()),
            amask: u32::from_le_bytes(s[28..32].try_into().unwrap()),
            bpp: i32::from_le_bytes(s[32..36].try_into().unwrap()),
        };
        (h, &s[off..])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LOCK: Mutex<()> = Mutex::new(());

    fn ensure_zeitfakto() -> Shmem {
        match ShmemConf::new()
            .os_id("ZeitfaktoSharedMemory")
            .size(1024)
            .create()
        {
            Ok(s) => s,
            Err(_) => ShmemConf::new()
                .os_id("ZeitfaktoSharedMemory")
                .open()
                .expect("ZeitfaktoSharedMemory"),
        }
    }

    #[test]
    fn shm_write_image_roundtrip() {
        let _guard = LOCK.lock().unwrap();
        let _keep = ensure_zeitfakto();
        let mut ed = SharedMemoryEditor::open().expect("open");
        let px: Vec<u8> = (1u8..=16).collect();
        ed.write_image(2, 2, &px).expect("write");
        let (hdr, pix) = ed.image_header_and_pixels();
        assert_eq!((hdr.width, hdr.height, hdr.pitch, hdr.bpp), (2, 2, 8, 32));
        assert_eq!(&pix[..16], &px[..]);
    }

    #[test]
    fn shm_write_image_rejects_bad_input() {
        let _guard = LOCK.lock().unwrap();
        let _keep = ensure_zeitfakto();
        let mut ed = SharedMemoryEditor::open().expect("open");
        assert!(ed.write_image(2, 2, &[0u8; 4]).is_err());
        assert!(ed.write_image(0, 0, &[]).is_err());
        assert!(ed.write_image(4096, 4097, &[]).is_err());
    }
}
