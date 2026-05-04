//! no-dangerously-set-inner-html — XSS vector.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-dangerously-set-inner-html",
    description: "`dangerouslySetInnerHTML` is an XSS vector.",
    remediation: "Remove the dangerouslySetInnerHTML prop. If you must \
                  render HTML, sanitize it with DOMPurify first and add a \
                  comment explaining the content's provenance.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    // Only applies to TSX/JSX — plain TS files don't have JSX.
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Tsx,
            Backend::Oxc(Box::new(oxc_typescript::Check)),
        )],
    }
}
