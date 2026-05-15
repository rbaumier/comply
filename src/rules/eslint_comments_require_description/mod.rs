//! eslint-comments-require-description — `eslint-disable` requires justification.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "eslint-comments-require-description",
    description: "`// eslint-disable-*` without a justification accumulates as silent tech debt.",
    remediation: "Add a justification after `--`: `// eslint-disable-next-line rule-id -- reason`. Same convention as comply's `// comply-ignore: rule — reason`.",
    severity: Severity::Warning,
    doc_url: Some("https://eslint-community.github.io/eslint-plugin-eslint-comments/rules/require-description.html"),
    categories: &["lint-comments"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
