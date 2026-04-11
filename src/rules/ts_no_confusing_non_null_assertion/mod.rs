//! ts-no-confusing-non-null-assertion — flag `a! == b` (looks like `a !== b`).

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-confusing-non-null-assertion",
    description: "`a! == b` looks confusingly like `a !== b`.",
    remediation: "Remove the `!` or wrap the left side in parentheses: `(a!) == b`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
