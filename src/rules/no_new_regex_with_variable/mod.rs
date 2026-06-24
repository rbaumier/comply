//! no-new-regex-with-variable — ReDoS risk.
//!
//! ReDoS is exploitable only when a running service feeds attacker-controlled
//! input to the regex (a crafted pattern freezes the event loop via exponential
//! backtracking). Test files (`skip_in_test_dir`) have no such attack surface:
//! a dynamic regex there matches a fixture-derived error message and never
//! ships, so the harm is production-only — mirroring the Rust backend, which
//! already exempts `tests/` and `#[test]` code. Production `new RegExp(variable)`
//! in its own (non-test) file is still flagged.

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

    skip_in_test_dir: true,
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
