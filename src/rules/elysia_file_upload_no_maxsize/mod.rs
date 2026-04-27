//! elysia-file-upload-no-maxsize

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-file-upload-no-maxsize",
    description: "`t.File()` / `t.Files()` without `maxSize` — uncapped file uploads can DoS the server.",
    remediation: "Set `maxSize: '5m'` (or another bound) on file upload schemas.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
