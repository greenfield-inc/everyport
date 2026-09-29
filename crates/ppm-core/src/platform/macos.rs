//! Macos implementation of [`Platform`]. Owned by the platform lane for this OS.

use super::{Listener, MemoryStats, Platform, ProcDetails, ProcInfo, ProcUsage};
use crate::protocol::ProcRef;
use std::io;

pub struct Macos;

impl Macos {
    pub fn new() -> Self {
        Self
    }
}

impl Platform for Macos {
    fn listeners(&self) -> io::Result<Vec<Listener>> {
        Ok(Vec::new())
    }
    fn processes(&self) -> io::Result<Vec<ProcInfo>> {
        Ok(Vec::new())
    }
    fn details(&self, _pid: u32, _env_keys: &[&str]) -> Option<ProcDetails> {
        None
    }
    fn usage(&self, _pid: u32) -> Option<ProcUsage> {
        None
    }
    fn memory(&self) -> MemoryStats {
        MemoryStats::default()
    }
    fn cpu_percent(&self) -> f32 {
        0.0
    }
    fn signal(&self, _target: ProcRef, _force: bool) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}
