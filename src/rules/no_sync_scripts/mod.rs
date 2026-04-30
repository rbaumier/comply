//! no-sync-scripts
//!
//! Flags `<script src="...">` elements that are neither `async` nor
//! `defer`. Synchronous external scripts block HTML parsing and delay
//! First Contentful Paint. Inline scripts (no `src`) are ignored —
//! they have different perf tradeoffs.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-sync-scripts",
    description: "External `<script src>` must set `async` or `defer` to avoid blocking parsing.",
    remediation: "Add `async` (order-independent) or `defer` (order-preserving) to the `<script>`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
