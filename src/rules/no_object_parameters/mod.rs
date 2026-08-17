//! no-object-parameters — flag a function parameter annotated `object`.
//!
//! `object` accepts every non-primitive value and exposes no property, so the
//! callee must narrow before it reads a field and the caller drops whatever
//! named type it held. A parenthesis and a union member keep the keyword in the
//! parameter's own annotation; any other type node around it is a different
//! position owned by another rule.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-object-parameters",
    description: "A parameter annotated `object` accepts every non-primitive value and exposes no property.",
    remediation: "Accept a named domain type, and parse external input at its I/O boundary before the call.",
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
