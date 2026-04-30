//! max-call-chain-depth

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "max-call-chain-depth",
    description: "Deeply nested function calls like f(g(h(i(x)))) are hard to debug.",
    remediation: "Extract intermediate variables to flatten the call stack.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
