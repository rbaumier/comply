//! react-jsx-no-comment-textnodes — comments as JSX text children.

mod text;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-comment-textnodes",
    description: "Comments placed as JSX text children are rendered as literal text.",
    remediation: "Use `{/* comment */}` for JSX comments, not `// comment` or \
                  `/* comment */` as bare text. Without braces, the comment \
                  syntax is rendered as visible text in the DOM.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, typescript).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(text::Check))));
    RuleDef { meta: META, backends }
}
