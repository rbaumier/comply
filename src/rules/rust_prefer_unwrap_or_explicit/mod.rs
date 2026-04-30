//! rust-prefer-unwrap-or-explicit — ban `.unwrap_or_default()`.
//!
//! `.unwrap_or_default()` hides the effective fallback value from the
//! reader: to know what happens on `None`/`Err`, they have to look up
//! the `Default` impl for the receiver's type. That extra hop makes
//! local reading harder and easy to get wrong at review time.
//!
//! The fix is to state the fallback explicitly at the call site via
//! `.unwrap_or(<value>)` or `.unwrap_or_else(|| <expr>)`. Tests are
//! exempted (test code often reaches for the shortest form).
//!
//! This rule is independent from `rust-no-unwrap`, which flags bare
//! `.unwrap()` / `.expect(...)`. The two are cumulable.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-prefer-unwrap-or-explicit",
    description: "Ban `.unwrap_or_default()`; require an explicit fallback value.",
    remediation: "Replace `.unwrap_or_default()` with `.unwrap_or(0)` / \
                  `.unwrap_or(String::new())` / `.unwrap_or_else(|| vec![])` — \
                  whatever the default is for this type, make it visible at \
                  the call site.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
