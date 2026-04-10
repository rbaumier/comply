//! rust-undocumented-unsafe — every unsafe block needs a SAFETY comment.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-undocumented-unsafe",
    description: "Every `unsafe` block must have a `// SAFETY:` comment.",
    remediation: "Add a `// SAFETY: ...` comment above every `unsafe { ... }` \
                  block explaining the invariants that make the unsafe code \
                  sound. The comment is what future debuggers will reach for \
                  when memory corruption shows up.",
    severity: Severity::Error,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
