mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "import-namespace",
    description: "Namespace import member must exist in the source module exports.",
    remediation: "Check that the accessed property is actually exported by the source module, \
                  or switch to a named import.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/namespace.md",
    ),
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
