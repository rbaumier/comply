//! rust-no-println-in-library — library code must use tracing, not println.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-println-in-library",
    description: "Library code must use tracing, not `println!` / `eprintln!`.",
    remediation: "Replace `println!` with `tracing::info!` / `tracing::debug!` \
                  and add structured fields. Library consumers configure the \
                  tracing subscriber; they cannot redirect `println!`. Enable \
                  `clippy::print_stdout` and `clippy::print_stderr`.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![],
    }
}
