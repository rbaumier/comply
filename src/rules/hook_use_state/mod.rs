mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "hook-use-state",
    description: "useState destructuring must follow `[value, setValue]` naming.",
    remediation: "Rename the setter to `set` + PascalCase of the state variable name.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/hook-use-state.md",
    ),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
