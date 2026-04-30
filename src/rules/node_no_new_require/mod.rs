//! node-no-new-require

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "node-no-new-require",
    description: "`new require('...')` is almost always a bug.",
    remediation: "Separate the `require` call from the `new` expression: `const Mod = require('...'); new Mod()`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
