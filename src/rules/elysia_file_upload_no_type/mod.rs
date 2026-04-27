//! elysia-file-upload-no-type

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-file-upload-no-type",
    description: "`t.File()` / `t.Files()` without `type` constraint — accepts any file type.",
    remediation: "Set `type: ['image/png', 'image/jpeg']` (or another allowlist) on file upload schemas.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
