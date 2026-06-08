//! no-unsanitized-method — flag unsafe HTML-injection method calls
//! (`insertAdjacentHTML`, `document.write`, `document.writeln`,
//! `setHTMLUnsafe`, `Range.createContextualFragment`) whose HTML argument
//! is not a static string literal.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unsanitized-method",
    description: "Calling DOM methods that parse HTML with a non-literal argument is an XSS vector.",
    remediation: "Avoid dynamic HTML injection, or sanitize input first",
    severity: Severity::Error,
    doc_url: Some(
        "https://cheatsheetseries.owasp.org/cheatsheets/DOM_based_XSS_Prevention_Cheat_Sheet.html",
    ),
    categories: &["security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

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
