//! no-boolean-flag-param — split boolean-flagged functions into two.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

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
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            // Rust: partial coverage via clippy::fn_params_excessive_bools.
            // Set `max-fn-params-bools = 0` in clippy.toml for strict parity.
            (Language::Rust, Backend::Clippy { lint: "clippy::fn_params_excessive_bools" }),
        ],
    }
}
