//! rust-hashmap-integer-key — `std`'s `HashMap`/`HashSet` keyed by an integer.
//!
//! The standard hasher is SipHash 1-3, chosen for HashDoS resistance on
//! attacker-controlled keys. An integer key is neither attacker-shaped nor
//! long: the hash costs more than the lookup it serves, and the cost is paid
//! on every `get` and `insert`.
//!
//! `rustc_hash::FxHashMap` (or `ahash::AHashMap`) keeps the same API and
//! swaps the hasher. Keys that *are* attacker-controlled — strings from a
//! request, user identifiers parsed from input — must keep SipHash, which is
//! why the rule only fires on integer keys.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-hashmap-integer-key",
    description: "`std::collections::HashMap`/`HashSet` with an integer key pays for SipHash's DoS resistance on a key that needs none.",
    remediation: "Swap the hasher: `rustc_hash::FxHashMap<K, V>` (or `ahash::AHashMap`) in place of `HashMap<K, V>`. \
                  Change the constructor too — `FxHashMap::default()` replaces `HashMap::new()`, and \
                  `FxHashMap::with_capacity_and_hasher(n, Default::default())` replaces `HashMap::with_capacity(n)`. \
                  Keep `std`'s `HashMap` when the key comes from untrusted input, where SipHash is the point.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "performance"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
