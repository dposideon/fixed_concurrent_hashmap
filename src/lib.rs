// Fixed Capacity Per Entry Locking Data Structure
mod errors;
mod spinlock;

use crate::errors::*;
use crate::spinlock::*;
use std::cell::UnsafeCell;
use std::ptr;
use std::sync::atomic::{AtomicU8, Ordering};

const EMPTY: u8 = 0;
const OCCUPIED: u8 = 1;
const TOMBSTONE: u8 = 2;
const INSERTING: u8 = 3;

pub type ID = [u8; 32];

// State Transitions:
//  EMPTY -> INSERTING -> OCCUPIED
//  TOMBSTONE -> INSERTING -> OCCUPIED
//  OCCUPIED -> TOMBSTONE

#[repr(C, align(64))]
pub struct Entry<V> {
    state: AtomicU8,
    hash: UnsafeCell<u64>,
    key: UnsafeCell<ID>,
    lock: SpinLock,
    value: UnsafeCell<V>,
}

pub struct Map<V> {
    mask: usize,
    entries: Box<[Entry<V>]>,
}

// SAFETY: Sync is managed manually:
//  - AtomicU8 state with Acquire/Release ordering guards visibility
//      of hash/key (Write once before occupied is published)
//  - Per entry SpinLock guards all mutations to value
unsafe impl<V: Send> Sync for Map<V> {}
unsafe impl<V: Send> Send for Map<V> {}

impl<V: Default> Map<V> {
    pub fn with_capacity_pow2(capacity: usize) -> Result<Self, CreateError> {
        if !capacity.is_power_of_two() {
            return Err(CreateError::InvalidSize);
        }

        let entries = (0..capacity)
            .map(|_| Entry {
                state: EMPTY.into(),
                hash: UnsafeCell::new(0u64),
                key: UnsafeCell::new([0u8; 32]),
                lock: SpinLock::new(),
                value: UnsafeCell::new(V::default()),
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Ok(Self {
            mask: capacity - 1,
            entries,
        })
    }

    pub fn find(&self, id: &ID) -> Option<&Entry<V>> {
        let hash = hash_id(id);
        let mut idx = (hash as usize) & self.mask;

        for _ in 0..self.entries.len() {
            let entry = &self.entries[idx];
            let state = entry.state.load(Ordering::Acquire);

            match state {
                EMPTY => return None,
                OCCUPIED => {
                    // SAFETY: Acquire load on state pairs
                    // Release store after writes in insert
                    // Data is fully initialized for OCCUPIED entry
                    let (k, h) = unsafe { (&*entry.key.get(), *entry.hash.get()) };

                    if h == hash && *k == *id {
                        return Some(entry);
                    }
                }
                TOMBSTONE | INSERTING => {}
                _ => {
                    eprintln!(
                        "State invariant violated:\nState: {}\nIndex: {}",
                        state, idx
                    );
                    return None;
                }
            }

            idx = (idx + 1) & self.mask;
        }

        None
    }

    pub fn insert(&self, id: ID, value: V) -> Result<(), InsertError> {
        let hash = hash_id(&id);

        for _ in 0..self.entries.len() {
            let mut idx = (hash as usize) & self.mask;
            let mut first_tombstone: Option<usize> = None;

            let mut found_empty = false;

            'inner: for _ in 0..self.entries.len() {
                let entry = &self.entries[idx];
                let state = entry.state.load(Ordering::Acquire);

                match state {
                    EMPTY => {
                        found_empty = true;
                        break 'inner;
                    }
                    OCCUPIED => {
                        let (k, h) = unsafe { (&*entry.key.get(), *entry.hash.get()) };
                        if h == hash && *k == id {
                            return Err(InsertError::Exists);
                        }
                    }
                    TOMBSTONE => {
                        if first_tombstone.is_none() {
                            first_tombstone = Some(idx);
                        }
                    }
                    INSERTING => {}
                    _ => {
                        eprintln!(
                            "State invariant violated:\nState: {}\nIndex: {}",
                            state, idx
                        );
                    }
                }

                idx = (idx + 1) & self.mask;
            }

            let (target_idx, expected) = if let Some(t) = first_tombstone {
                (t, TOMBSTONE)
            } else if found_empty {
                (idx, EMPTY)
            } else {
                return Err(InsertError::Full);
            };

            let target = &self.entries[target_idx];

            if target
                .state
                .compare_exchange(expected, INSERTING, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                unsafe {
                    *target.hash.get() = hash;
                    *target.key.get() = id;
                    *target.value.get() = value;
                }
                target.state.store(OCCUPIED, Ordering::Release);
                return Ok(());
            }
        }

        Err(InsertError::Full)
    }

    pub fn delete(&self, id: ID) -> DeleteResult {
        if let Some(entry) = self.find(&id) {
            let guard = entry.lock.lock();

            let state = entry.state.load(Ordering::Acquire);
            if state != OCCUPIED {
                return DeleteResult::NotFound;
            }

            let k = unsafe { &*entry.key.get() };
            if *k != id {
                return DeleteResult::NotFound;
            }

            entry.state.store(TOMBSTONE, Ordering::Release);
            drop(guard);
            DeleteResult::Deleted
        } else {
            DeleteResult::NotFound
        }
    }

    pub fn update(&self, id: &ID, f: impl FnOnce(&mut V)) -> UpdateResult {
        if let Some(entry) = self.find(id) {
            let _guard = entry.lock.lock();
            if entry.state.load(Ordering::Acquire) != OCCUPIED {
                return UpdateResult::NotFound;
            }

            unsafe { f(&mut *entry.value.get()) };

            UpdateResult::Updated
        } else {
            UpdateResult::UpdateFailed
        }
    }
}

#[inline]
fn hash_id(key: &ID) -> u64 {
    let ptr = key.as_ptr() as *const u64;

    let a = unsafe { ptr::read_unaligned(ptr).to_be() };
    let b = unsafe { ptr::read_unaligned(ptr.add(1)).to_be() };
    let c = unsafe { ptr::read_unaligned(ptr.add(2)).to_be() };
    let d = unsafe { ptr::read_unaligned(ptr.add(3)).to_be() };

    let mut x = a ^ b.rotate_left(13);
    x ^= c.rotate_left(29);
    x ^= d.rotate_left(47);
    x.wrapping_mul(0x9E3779B97F4A7C15)
}
