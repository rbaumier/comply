//! no-unknown-parameters — flag a function parameter annotated `unknown`.
//!
//! An `unknown` parameter is a deferred parse: the callee cannot use the value
//! without narrowing it, and a caller that already held a type loses it at the
//! call boundary. A parameter named `cause` is exempt — `new Error(msg, {
//! cause })` genuinely carries an unknown.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unknown-parameters",
    description: "An `unknown` parameter defers the parse onto every caller.",
    remediation: "Accept a named domain type, and run the parser at the I/O boundary instead.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
