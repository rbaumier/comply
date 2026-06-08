//! rust-iter-count-vs-len — `.iter().count()` walks the whole collection.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-iter-count-vs-len",
    description: "`.iter().count()` walks the whole collection to compute its length.",
    remediation: "Use `.len()` directly on the collection — `Vec`, slices, \
                  `VecDeque`, `String`, `HashMap`, and `HashSet` all expose \
                  it in O(1). `.count()` consumes an iterator linearly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
