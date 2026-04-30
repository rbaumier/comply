//! node-no-sync

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "node-no-sync",
    description: "Synchronous Node.js methods block the event loop.",
    remediation: "Use the asynchronous variant (e.g. `readFile` instead of `readFileSync`) or `fs.promises`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
