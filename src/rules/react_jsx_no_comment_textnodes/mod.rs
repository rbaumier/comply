//! react-jsx-no-comment-textnodes — comments as JSX text children.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

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
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
