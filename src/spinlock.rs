use std::sync::atomic::AtomicBool;

pub struct SpinLock {
    locked: AtomicBool,
}

pub struct SpinGuard<'a> {
    lock: &'a SpinLock,
}

impl SpinLock {
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
        }
    }

    pub fn lock(&self) -> SpinGuard<'_> {
        loop {
            if !self.locked.swap(true, std::sync::atomic::Ordering::Acquire) {
                return SpinGuard { lock: self };
            }

            while self.locked.load(std::sync::atomic::Ordering::Relaxed) {
                std::hint::spin_loop();
            }
        }
    }
}

impl Drop for SpinGuard<'_> {
    fn drop(&mut self) {
        self.lock
            .locked
            .store(false, std::sync::atomic::Ordering::Release);
    }
}
