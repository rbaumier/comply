//! eslint-comments-disable-enable-pair — every `eslint-disable` needs an `eslint-enable`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "eslint-comments-disable-enable-pair",
    description: "An `eslint-disable` block comment without a matching `eslint-enable` leaves \
                  the rule disabled for the rest of the file — usually a copy-paste oversight.",
    remediation: "Add `/* eslint-enable */` (or `/* eslint-enable <rule> */`) at the point the \
                  exception should end. For single-line exceptions, use `eslint-disable-next-line` \
                  or `eslint-disable-line` instead.",
    severity: Severity::Warning,
    doc_url: Some("https://mysticatea.github.io/eslint-plugin-eslint-comments/rules/disable-enable-pair.html"),
    categories: &["eslint-comments"],
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
