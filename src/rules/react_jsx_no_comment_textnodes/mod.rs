//! react-jsx-no-comment-textnodes — comments as JSX text children.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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
    let backends = vec![
        (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
    ];
    RuleDef {
        meta: META,
        backends,
    }
}
