//! rust-select-without-biased — `tokio::select!` defaults to a
//! pseudo-random poll order. For shutdown / cancellation patterns the
//! `biased;` directive is what you want, otherwise the cancel branch
//! starves intermittently and you get flaky tests under load.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-select-without-biased",
    description: "`tokio::select!` without a `biased;` directive picks branches in random order.",
    remediation: "Add `biased;` as the first line of the `select!` body. \
                  Without it tokio polls the branches in a pseudo-random \
                  order, which means a shutdown / cancel branch can be \
                  starved by a noisy data branch under load. With \
                  `biased;` the branches are polled top-down so cancellation \
                  always wins.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust", "concurrency", "tokio"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
