//! rust-prefer-cow — pub fns taking an owned `String` parameter force
//! every caller to allocate, even when they already hold a `&'static str`
//! literal or a borrowed slice. `Cow<'_, str>` (with `impl Into<Cow<str>>`)
//! lets callers pass either a borrow or an owned value, and only clones
//! when the function actually needs ownership.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-prefer-cow",
    description: "Public functions taking an owned `String` force callers to allocate — prefer `Cow<'_, str>` or `&str`.",
    remediation: "Change `pub fn foo(s: String)` to `pub fn foo(s: impl Into<Cow<'_, str>>)` when the function only sometimes needs ownership, or to `&str` when it never does.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
