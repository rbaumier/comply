//! no-invalid-fetch-options

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-invalid-fetch-options",
    description: "`fetch()` / `new Request()` with `body` on a GET or HEAD request is invalid.",
    remediation: "Remove the `body` property or change the method to POST/PUT/PATCH.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
