mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-result-require-gen-for-chains",
    description: "Require Result.gen + yield* when chaining 2+ Results instead of nested .andThen.",
    remediation: "Rewrite the chain using Result.gen(function* () { const a = yield* ...; const b = yield* ...; }).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["better-result"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
