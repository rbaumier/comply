//! security-no-sri-missing

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "security-no-sri-missing",
    description: "`<script src=\"https://...\">` and `<link rel=\"stylesheet\">` from third-party origins must carry an `integrity` attribute (Subresource Integrity).",
    remediation: "Add an `integrity=\"sha384-…\"` attribute (and typically `crossOrigin=\"anonymous\"`) so the browser rejects tampered assets.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
