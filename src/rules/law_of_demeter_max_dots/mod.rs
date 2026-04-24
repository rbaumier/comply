mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "law-of-demeter-max-dots",
    description: "Member access chain reaches more than 2 levels into a dependency.",
    remediation: "Ask the collaborator for a higher-level method instead of reaching into its internals (`a.doX()`, not `a.b.c.x()`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
