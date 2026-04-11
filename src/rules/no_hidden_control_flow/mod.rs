//! no-hidden-control-flow

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-hidden-control-flow",
    description: "3+ decorators stacked on a single function/class hide control flow.",
    remediation: "Reduce the decorator stack to 2 or fewer. Each decorator adds invisible control flow — stacking 3+ makes the execution path hard to reason about. Compose decorators into a single higher-level one or use explicit middleware.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
