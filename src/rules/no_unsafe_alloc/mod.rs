//! no-unsafe-alloc
//!
//! The harm is uninitialized heap memory escaping to an attacker in production
//! (leaking prior heap contents). Test files (`skip_in_test_dir`) never ship
//! such a buffer — there `allocUnsafe` builds throwaway fixtures whose content
//! is irrelevant by design (e.g. size-varying buffers fed to a validator to
//! assert it rejects invalid sizes). A production use of `allocUnsafe` /
//! `new Buffer(size)` is still flagged in its own (non-test) file.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unsafe-alloc",
    description: "Avoid `Buffer.allocUnsafe()` and `new Buffer(size)` — they return uninitialized memory.",
    remediation: "Use `Buffer.alloc(size)` for zero-filled buffers or `Buffer.from(data)` for initialized data. `allocUnsafe` / `new Buffer(size)` can leak prior heap contents.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
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
