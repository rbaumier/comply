//! no-unknown-type-aliases — flag a type alias that resolves to `unknown`.
//!
//! `type Json = unknown` reads like a domain type at every use site and is
//! `unknown` everywhere. The alias buys a name and pays with the type system:
//! callers see `Json`, believe something was decided, and still have to
//! narrow.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unknown-type-aliases",
    description: "A type alias resolving to `unknown` names a decision that was never made.",
    remediation: "Name the type the value actually has, and narrow at the parsing boundary.",
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
