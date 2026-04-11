//! node-prefer-promises-dns

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "node-prefer-promises-dns",
    description: "Callback-based `dns.*` methods are discouraged.",
    remediation: "Use `dns.promises.*` or import from `dns/promises` instead of callback-based `dns` methods.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
