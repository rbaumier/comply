//! no-abusive-eslint-disable

mod oxc_typescript;
mod rust;
mod text;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-abusive-eslint-disable",
    description: "`eslint-disable` without specifying rules silences everything — too broad.",
    remediation: "Specify the exact rules to disable: `eslint-disable-next-line no-console`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

/// The eslint disable directives that require a rule list.
const DIRECTIVES: &[&str] = &[
    "eslint-disable-next-line",
    "eslint-disable-line",
    "eslint-disable",
];

/// Returns true if the comment text contains an eslint-disable directive
/// without specifying which rule(s) to disable.
pub(crate) fn is_abusive_disable(text: &str) -> bool {
    for directive in DIRECTIVES {
        if let Some(pos) = text.find(directive) {
            let end = pos + directive.len();
            // If the char right after the directive is a hyphen, this is a
            // longer directive (e.g. `eslint-disable` inside
            // `eslint-disable-next-line`). Skip — the longer directive will
            // match on its own iteration.
            if text.as_bytes().get(end) == Some(&b'-') {
                continue;
            }
            let after_trimmed = text[end..].trim();
            if after_trimmed.is_empty() || after_trimmed == "*/" || after_trimmed == "-->" {
                return true;
            }
            if after_trimmed.starts_with("--") {
                return true;
            }
            if let Some(first) = after_trimmed.chars().next()
                && !first.is_ascii_alphabetic()
                && first != '@'
            {
                return true;
            }
        }
    }
    false
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
