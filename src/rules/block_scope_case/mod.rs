//! block-scope-case

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "block-scope-case",
    description: "`case` clause contains a lexical declaration that is not wrapped in a block.",
    remediation: "Wrap the case body in braces: `case X: { const y = ...; break; }`. Otherwise the binding leaks into sibling cases and can trigger TDZ errors.",
    severity: Severity::Warning,
    doc_url: Some("https://sonarsource.github.io/rspec/#/rspec/S1301"),
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
