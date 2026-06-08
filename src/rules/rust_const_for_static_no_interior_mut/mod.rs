//! rust-const-for-static-no-interior-mut — `static FOO: T = literal;` with
//! no interior mutability should be a `const`. `const` is inlined at every
//! use site and never takes an address; `static` reserves a single memory
//! location and is only required when interior mutability or a stable
//! address matters.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-const-for-static-no-interior-mut",
    description: "Use `const` instead of `static` for plain-literal values without interior mutability.",
    remediation: "Change `static FOO: T = …;` to `const FOO: T = …;` when \
                  `T` has no interior mutability (`Cell`, `Mutex`, `OnceLock`, …) \
                  and the value is a literal or `const fn` expression. \
                  `const` inlines at every use site; `static` reserves a \
                  fixed address you don't need.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
