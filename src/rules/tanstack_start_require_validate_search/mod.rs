mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-require-validate-search",
    description: "Routes calling `Route.useSearch()` must define `validateSearch:` on the route.",
    remediation: "Add `validateSearch: z.object({ ... })` to the `createFileRoute()` options.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
