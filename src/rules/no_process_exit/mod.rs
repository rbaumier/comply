//! no-process-exit

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-process-exit",
    description: "`process.exit()` terminates abruptly — throw an error instead.",
    remediation: "Replace `process.exit()` with `throw new Error(...)`. Only use `process.exit()` in CLI entry points.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
