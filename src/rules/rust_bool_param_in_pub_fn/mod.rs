//! rust-bool-param-in-pub-fn — one `bool` in a public signature is already a
//! flag.
//!
//! `walk(path, true)` is unreadable at the call site: nothing in the expression
//! says what `true` selects, so the reader has to open the callee. An enum with
//! two named variants carries the meaning to the caller — `walk(path,
//! Recursion::Recursive)` — and lets a third case appear later without changing
//! the arity.
//!
//! Scope is the public surface only, and one `bool` is enough. The Rust half of
//! `no-boolean-flag-param` defers to `clippy::fn_params_excessive_bools`, which
//! fires at three or more `bool` parameters whatever the visibility.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-bool-param-in-pub-fn",
    description: "A `bool` parameter on a public function is a flag callers read as a bare `true`.",
    remediation: "Replace the `bool` parameter with a two-variant enum named \
                  after the choice it encodes: \
                  `pub enum Recursion { Recursive, Flat }`, then \
                  `pub fn walk(path: &Path, recursion: Recursion)`. The call \
                  site turns from `walk(p, true)` into \
                  `walk(p, Recursion::Recursive)`, and a third mode can be added \
                  without changing the signature's shape. Unlike \
                  `no-boolean-flag-param` (Rust: `clippy::fn_params_excessive_bools`, \
                  three or more `bool`s, any visibility), a single `bool` on a \
                  public signature is enough here — it is the call site, not the \
                  parameter count, that is unreadable.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "api"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
