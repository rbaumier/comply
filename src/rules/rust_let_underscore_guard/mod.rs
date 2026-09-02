//! rust-let-underscore-guard — `let _ = <guard>;` binds nothing, so the RAII
//! guard the expression just produced is dropped at the end of that very
//! statement. The mutex is unlocked again, the tracing span closes, the temp
//! directory is erased — all before the code the author meant to protect runs.
//!
//! The rule fires on a `let _ = …;` whose value is a zero-argument
//! `lock` / `try_lock` / `read` / `write` / `enter` / `entered` call, or a
//! temp-path constructor (`TempDir::new`, `NamedTempFile::new`, `tempdir`),
//! reached through any number of `?`, `.await`, `.unwrap()` and `.expect(..)`
//! hand-offs. Binding the guard to a name — `let _guard = …` — is the fix.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-let-underscore-guard",
    description: "`let _ = <guard>;` drops the guard immediately — the lock, span or temp directory it protects is gone before the next statement.",
    remediation: "Bind the guard to a named variable that lives as long as the critical section: `let _guard = m.lock().unwrap();`. Keep `let _ = …` for values you really do want dropped now.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "correctness"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
