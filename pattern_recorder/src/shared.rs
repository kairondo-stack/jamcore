// Shared Memory: Boost-kompatibles ZeitfaktoSharedMemory (SharedData, 1024 Bytes)
use shared_memory::{Shmem, ShmemConf};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SharedData {
    pub zeitfakto: f32,
    pub variable2: f32,
    pub variable3: f32,
    pub dauer: f32,
    pub bpm: f32,
    pub p1: bool,
    pub p2: bool,
    pub p3: bool,
}

pub const HH_SIZE: usize = 1024;

impl Default for SharedData {
    fn default() -> Self {
        SharedData {
            zeitfakto: 1.0,
            variable2: 1.0,
            variable3: 1.0,
            dauer: 4.0,
            bpm: 120.0,
            p1: false,
            p2: false,
            p3: false,
        }
    }
}

/// Entspricht dem C++-Konstruktor: remove() (ignoriert Fehler), dann create_only.
pub fn create_zeitfakto_shm() -> Result<Shmem, String> {
    unsafe {
        libc::shm_unlink(b"ZeitfaktoSharedMemory\0".as_ptr() as *const libc::c_char);
    }
    ShmemConf::new()
        .os_id("ZeitfaktoSharedMemory")
        .size(HH_SIZE)
        .create()
        .map_err(|e| format!("Failed to create shared memory: {e}"))
}