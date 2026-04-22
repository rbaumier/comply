//! no-shell-exec

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-shell-exec",
    description: "`exec()` with template literals or `shell: true` is a command injection vector.",
    remediation: "Use `execFile()` with a fixed command and an args array, never `shell: true`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
