//! no-new-regex-with-variable — ReDoS risk.
//!
//! ReDoS is exploitable only when a running service feeds attacker-controlled
//! input to the regex (a crafted pattern freezes the event loop via exponential
//! backtracking).
//! Test files have no such attack surface.
//! A dynamic regex there matches a fixture-derived error message.
//! It never ships, so the harm is production-only.
//! The TypeScript backend skips test files by name.
//! The Rust backend exempts test-only code, as Cargo builds it.
//! A Rust file named after tests ships.
//! Production `new RegExp(variable)` is still flagged.

mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-new-regex-with-variable",
    description: "`new RegExp(variable)` enables ReDoS attacks.",
    remediation: "Replace dynamic regex construction with a literal regex \
                  or a vetted safe-regex library. User-controlled patterns \
                  can trigger exponential backtracking and freeze the event loop.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],

    // A Rust file named after tests still ships.
    // The other backends skip `in_test_dir` themselves.
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
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
