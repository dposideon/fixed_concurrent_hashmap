# fixed-concurrent-map

A fixed-capacity, concurrent hash map with per-entry locking and
lock-free reads. Designed for performance-critical scenarios where
low-latency access to shared state is essential.

## Design

- **Fixed capacity** — No resizing. Capacity must be a power of 2,
  enabling bitwise index masking instead of modulo division.
- **Zero allocation after init** — The map, all entries, and all
  values are allocated once at construction. No heap allocation
  occurs during `insert`, `find`, `update`, or `delete`. The `V`
  generic is initialized with `Default` upfront for this reason.
- **Lock-free reads** — `find` uses only atomic loads with
  acquire/release ordering. No mutex, no spinlock.
- **Per-entry spinlocks** — `update` and `delete` lock only the
  affected entry, minimizing contention.
- **Atomic state machine** — Each entry transitions through
  `EMPTY → INSERTING → OCCUPIED → TOMBSTONE` using CAS operations.
  `insert` requires no lock; the `INSERTING` state acts as an
  implicit exclusive claim on the slot.
- **Cache-line aligned entries** — Each `Entry` is aligned to 64
  bytes to avoid false sharing between cores.
- **32-byte key** — The key type is a fixed `[u8; 32]`, Designed for 
  high-performance systems that use fixed-width cryptographic identifiers 
  (e.g., content hashes, Merkle roots, object IDs). The hash function 
  exploits the fixed width to
  operate branchlessly with no hashing library overhead.

## Usage

```rust
use fixed_map::Map;

let map: Map<u64> = Map::capacity_with_pow2(1024).unwrap();

let key: [u8; 32] = [0u8; 32];

// Insert
map.insert(key, 42).unwrap();

// Read
if let Some(entry) = map.find(&key) {
    // use entry
}

// Update
map.update(&key, |v| *v += 1);

// Delete
map.delete(key);
```

## Known Limitations & Future Work

### Duplicate key safety

Concurrent inserts of the **same key** are not protected against.
When two threads insert the same key simultaneously, one thread's
scan may skip the other's in-progress slot (state `INSERTING`),
resulting in duplicate entries.

This is safe when the caller can guarantee unique keys per insert
(e.g. key assignment is partitioned or serialized upstream). A
future release may add post-CAS duplicate detection with rollback.

### Tombstone cleanup

Deleted entries are marked as tombstones but the stored value is
**not** reset or dropped until the slot is reused by a future
insert. This is intentional — avoiding the extra write on delete
keeps the hot path fast. A future release may add an optional
`drop_on_delete` mode.

### No resize

The map does not grow. If the map fills up, inserts return
`InsertError::Full`. Choose capacity accordingly.

## [License](./LICENSE)

MIT
