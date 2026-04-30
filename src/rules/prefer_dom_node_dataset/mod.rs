//! prefer-dom-node-dataset

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-dataset",
    description: "Prefer `.dataset` over `.setAttribute('data-*')` / `.getAttribute('data-*')`.",
    remediation: "Replace `.setAttribute('data-foo', v)` with `.dataset.foo = v` and \
                  `.getAttribute('data-foo')` with `.dataset.foo`. The `dataset` API \
                  is cleaner and avoids string-based attribute manipulation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
