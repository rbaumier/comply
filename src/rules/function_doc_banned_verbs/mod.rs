mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "function-doc-banned-verbs",
    description: "Function docstring opens with a verb that paraphrases the implementation.",
    remediation: "Open the docstring with intent (`Ensure…`, `Return…`), not restatement (`Reads…`, `Iterates…`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
