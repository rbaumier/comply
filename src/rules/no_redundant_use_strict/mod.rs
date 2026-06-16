//! no-redundant-use-strict — flag `"use strict"` directives that have no effect.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-use-strict",
    description: "Redundant `\"use strict\"` directive.",
    remediation: "Remove the directive. ES modules and class bodies are always in strict mode, and an enclosing `\"use strict\"` already covers any nested scope, so the directive has no effect.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["suspicious"],

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
        ],
    }
}
