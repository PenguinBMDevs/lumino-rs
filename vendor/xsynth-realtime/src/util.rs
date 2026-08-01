use std::sync::atomic::{AtomicU64, Ordering};

pub struct ReadWriteAtomicU64(AtomicU64);

impl ReadWriteAtomicU64 {
    pub fn new(value: u64) -> Self {
        ReadWriteAtomicU64(AtomicU64::new(value))
    }

    pub fn read(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    pub fn write(&self, value: u64) {
        self.0.store(value, Ordering::Relaxed)
    }
}
