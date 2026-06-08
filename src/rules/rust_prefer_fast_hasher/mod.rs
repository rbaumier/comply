//! rust-prefer-fast-hasher — `HashMap`/`HashSet` keyed by primitive
//! integers pays SipHash's DoS-resistance cost for no reason when the
//! keys are trusted. A faster hasher (`ahash`, `rustc_hash::FxHashMap`)
//! is usually 2-5x faster on integer keys.
//!
//! Only flags the plain 2-arg `HashMap<K, V>` / 1-arg `HashSet<K>` form
//! (no explicit hasher). When the key is a primitive integer type
//! (`u8`..`u128`, `i8`..`i128`, `usize`, `isize`), suggest a faster
//! hasher. String/complex keys still benefit from SipHash's DoS
//! resistance, so those are left alone.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-prefer-fast-hasher",
    description: "`HashMap` / `HashSet` with integer keys defaults to the slower SipHash — use a faster hasher.",
    remediation: "Switch to `ahash::AHashMap`, `rustc_hash::FxHashMap`, or `HashMap<K, V, FxBuildHasher>` when keys are trusted integers (u32/u64/usize/i32/i64).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust", "performance"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
