//! elysia-file-magic-number

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-file-magic-number",
    description: "`z.file()` validates the MIME header — clients can forge it. Verify magic numbers via `fileType`.",
    remediation: "Pair `z.file()` with `.refine(buf => fileType(buf)?.mime === 'image/png')` or equivalent magic-number check.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
