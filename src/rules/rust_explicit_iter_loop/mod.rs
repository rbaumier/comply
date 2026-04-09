//! rust-explicit-iter-loop — iterator chains over raw index loops.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-explicit-iter-loop",
    description: "Use iterator chains, not raw index loops.",
    remediation: "Replace `for i in 0..vec.len() { vec[i] }` with \
                  `for x in &vec`. Iterator chains let the compiler \
                  vectorize the loop body and eliminate bounds checks. \
                  Enable `clippy::needless_range_loop` and \
                  `clippy::explicit_iter_loop`.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::Rust, Backend::Clippy { lint: "clippy::explicit_iter_loop" }),
            (Language::Rust, Backend::Clippy { lint: "clippy::needless_range_loop" }),
        ],
    }
}
