//! no-sync-scripts
//!
//! Flags `<script src="...">` elements that are neither `async` nor
//! `defer`. Synchronous external scripts block HTML parsing and delay
//! First Contentful Paint. Inline scripts (no `src`) are ignored —
//! they have different perf tradeoffs.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-sync-scripts",
    description: "External `<script src>` must set `async` or `defer` to avoid blocking parsing.",
    remediation: "Add `async` (order-independent) or `defer` (order-preserving) to the `<script>`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
