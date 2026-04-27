//! rust-format-args-in-log-macro — `info!("{}", format!(...))` is double-formatting.
//! `info!`/`debug!`/`warn!`/`error!`/`trace!` already accept format args
//! directly, so wrapping in `format!` allocates a `String` only to copy it.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-format-args-in-log-macro",
    description: "Don't pass `format!(...)` to log/tracing macros — pass the args directly.",
    remediation: "Replace `info!(\"{}\", format!(\"x = {}\", x))` with \
                  `info!(\"x = {}\", x)`. The log/tracing macros already \
                  accept the same format-args grammar as `format!`, so \
                  the inner `format!` allocates a `String` only to be \
                  copied into the log buffer.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
