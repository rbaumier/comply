//! serialize-javascript-no-unsafe — flag `serialize(x, { unsafe: true })`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "serialize-javascript-no-unsafe",
    description: "`serialize(value, { unsafe: true })` disables HTML escaping (XSS risk).",
    remediation: "Don't use unsafe option in serialize-javascript, it disables HTML escaping.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/yahoo/serialize-javascript#user-content-options"),
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
