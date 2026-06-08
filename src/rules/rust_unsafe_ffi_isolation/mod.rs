//! rust-unsafe-ffi-isolation — quarantine FFI into a named submodule.
//!
//! `extern "C"` / `extern "system"` blocks are the boundary where
//! Rust's safety guarantees stop applying. Putting them in a
//! dedicated `mod sys { … }` / `mod ffi { … }` makes that boundary
//! grep-able, keeps bindgen-style raw APIs out of the top of the
//! file, and funnels every unsafe cast through one audit surface.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-unsafe-ffi-isolation",
    description: "`extern \"C\"` blocks should be isolated inside a `mod sys`, `mod ffi`, or `mod raw` module.",
    remediation: "Move the `extern \"C\"` block into a dedicated submodule: `mod sys { extern \"C\" { ... } }`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
