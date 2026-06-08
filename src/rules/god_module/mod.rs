//! god-module — flag modules imported by a large fraction of the project.
//!
//! Centralisation smell: a single file that most of the codebase imports from
//! becomes a rebuild bottleneck, a merge-conflict hotspot, and a dependency
//! fanout that hides coupling. The threshold is configurable via
//! `[rules.god-module] threshold_percent` / `min_importers` in `comply.toml`.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "god-module",
    description: "Module is imported by a large fraction of the project — centralisation smell.",
    remediation: "Split the module into smaller, focused ones so importers only pull in what they need. High fan-in modules are rebuild and merge-conflict bottlenecks.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "imports"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
