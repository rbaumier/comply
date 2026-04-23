//! no-mocks-import

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-mocks-import",
    description: "Do not import directly from a `__mocks__` directory.",
    remediation: "Let Jest/Vitest auto-resolve mocks, don't import from __mocks__ directly",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
