//! react-jsx-no-target-blank — missing rel="noreferrer" with target="_blank".

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-target-blank",
    description: "`target=\"_blank\"` without `rel=\"noreferrer\"` is a security risk.",
    remediation: "Add `rel=\"noreferrer\"` (or `rel=\"noopener noreferrer\"`) when \
                  using `target=\"_blank\"`. Without it, the opened page can access \
                  `window.opener` and redirect the parent page.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

/// Whether a `rel` attribute value severs `window.opener` for a `target="_blank"` link.
///
/// The value is a space-separated token list (per the HTML spec). Either `noopener`
/// (which alone nulls `window.opener`) or `noreferrer` (which implies `noopener`)
/// closes the reverse-tabnabbing vector. Token order and unrelated tokens (e.g.
/// `nofollow`) are irrelevant; matching is case-insensitive.
fn rel_is_safe(value: &str) -> bool {
    value.split_ascii_whitespace().any(|token| {
        token.eq_ignore_ascii_case("noopener") || token.eq_ignore_ascii_case("noreferrer")
    })
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
