//! eslint-comments-no-unlimited-disable — `eslint-disable` without rule list.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "eslint-comments-no-unlimited-disable",
    description: "`// eslint-disable` / `// eslint-disable-next-line` without a rule list disables every rule.",
    remediation: "Name the rules you intend to disable: `// eslint-disable-next-line rule-a, rule-b`. Same for comply: `// comply-ignore: rule-id — reason`.",
    severity: Severity::Warning,
    doc_url: Some("https://eslint-community.github.io/eslint-plugin-eslint-comments/rules/no-unlimited-disable.html"),
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
