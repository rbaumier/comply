//! imports-first

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "imports-first",
    description: "Import statements must appear before any other code.",
    remediation: "Move all import/require statements to the top of the file, before any non-import code (except directives like `'use strict'`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
