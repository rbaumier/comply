//! no-unknown-returns — flag a declared return type that resolves to
//! `unknown`.
//!
//! A function that declares `unknown` hands its caller an unparsed value.
//! Every call site re-narrows it, and what the function knew about the value
//! it produced is dropped at the boundary the function owns.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unknown-returns",
    description: "A declared `unknown` return hands the caller an unparsed value.",
    remediation: "Return a named domain type, and parse the value where it is produced.",
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
