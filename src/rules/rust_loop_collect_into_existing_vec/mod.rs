//! rust-loop-collect-into-existing-vec — `for x in src { dst.push(x); }`
//! is the long-form of `dst.extend(src)`. The `extend` form lets the iter
//! adapter pre-size `dst` via `size_hint` and reads better at the call site.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-loop-collect-into-existing-vec",
    description: "`for x in src { v.push(x); }` should be `v.extend(src)`.",
    remediation: "Use `v.extend(src)` (or `v.extend(src.into_iter().map(...))` \
                  if you need a transform). `extend` consults the iterator's \
                  `size_hint` and reserves capacity in one allocation; the \
                  loop form re-grows the `Vec` once per element.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
