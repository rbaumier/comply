//! no-unsafe-alloc

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-unsafe-alloc",
    description: "Avoid `Buffer.allocUnsafe()` and `new Buffer(size)` — they return uninitialized memory.",
    remediation: "Use `Buffer.alloc(size)` for zero-filled buffers or `Buffer.from(data)` for initialized data. `allocUnsafe` / `new Buffer(size)` can leak prior heap contents.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
