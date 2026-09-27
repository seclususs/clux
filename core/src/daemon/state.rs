//! Author: [Seclususs](https://github.com/seclususs)

use std::sync;

pub static SHUTDOWN_REQUESTED: sync::atomic::AtomicBool = sync::atomic::AtomicBool::new(false);

#[derive(Debug, Clone, Copy, Default)]
pub struct GlobalPressure {
    pub cpu_psi: f32,
    pub io_psi: f32,
    pub io_saturation: f32,
}

#[derive(Debug, Default)]
pub struct DaemonContext {
    pub pressure: GlobalPressure,
}

impl DaemonContext {
    pub fn new() -> Self {
        Self {
            pressure: GlobalPressure::default(),
        }
    }
}
