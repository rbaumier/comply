//! consistent-template-literal-escape

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "consistent-template-literal-escape",
    description: "Use `\\${` instead of `$\\{` to escape in template literals.",
    remediation: "Escape the dollar sign (`\\${`) rather than the opening brace (`$\\{`) or both (`\\$\\{`). This is the consistent way to prevent expression interpolation in template literals.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
