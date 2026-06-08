//! no-boolean-flag-param — split boolean-flagged functions into two.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-boolean-flag-param",
    description: "Boolean flag parameters hide two behaviors behind one signature.",
    remediation: "Split into two named functions. \
                  `sendNotification(msg, isUrgent)` → \
                  `sendUrgentNotification(msg)` + `sendNormalNotification(msg)`. \
                  A ternary or options object is not a fix — the boolean \
                  must disappear from the signature.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};
pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::Clippy { lint: "clippy::fn_params_excessive_bools" }),
        ],
    }
}
