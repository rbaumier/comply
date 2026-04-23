//! no-unsanitized-method — flag unsafe HTML-injection method calls
//! (`insertAdjacentHTML`, `document.write`, `document.writeln`,
//! `setHTMLUnsafe`, `Range.createContextualFragment`) whose HTML argument
//! is not a static string literal.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-unsanitized-method",
    description: "Calling DOM methods that parse HTML with a non-literal argument is an XSS vector.",
    remediation: "Avoid dynamic HTML injection, or sanitize input first",
    severity: Severity::Error,
    doc_url: Some(
        "https://cheatsheetseries.owasp.org/cheatsheets/DOM_based_XSS_Prevention_Cheat_Sheet.html",
    ),
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
