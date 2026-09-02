//! rust-mutex-over-atomic — a `Mutex` or `RwLock` wrapped around a single
//! `bool` or integer buys nothing the matching atomic does not already give.
//! The atomic reads, writes and read-modify-writes that scalar without ever
//! blocking a thread, without a poisoned-lock error path, and without the
//! guard's lifetime leaking into every signature that touches the value.
//!
//! The rule fires on `Mutex<T>` / `RwLock<T>` — std, tokio or parking_lot,
//! matched on the last `::` segment — where `T` is `bool`, `usize`, `isize`,
//! or a sized integer with an `AtomicX` counterpart, in any position: struct
//! field, local binding, `Arc<Mutex<bool>>`, return type.
//!
//! A composite payload (`Mutex<Option<bool>>`, `Mutex<(bool, u32)>`,
//! `Mutex<Vec<u8>>`), a float (no `AtomicF64` in std) and a file that also uses
//! a `Condvar` — where the mutex is the condvar's companion, not a container —
//! are all left alone.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-mutex-over-atomic",
    description: "`Mutex<bool>` / `Mutex<u32>` locks a thread to touch one scalar an atomic handles lock-free.",
    remediation: "Replace the lock with the matching atomic — `Mutex<bool>` → `AtomicBool`, `Mutex<usize>` → `AtomicUsize`, `Mutex<u32>` → `AtomicU32` — and swap `*guard` reads and writes for `load` / `store` / `fetch_add` / `compare_exchange` with an explicit `Ordering`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "performance"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
