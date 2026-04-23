//! html-require-explicit-size
//!
//! Flags `<img>` and `<video>` elements lacking explicit `width` and
//! `height`. Reserving space reduces Cumulative Layout Shift (CLS).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "html-require-explicit-size",
    description: "`<img>` and `<video>` must declare `width` and `height` to avoid layout shift.",
    remediation: "Add explicit `width` and `height` attributes (or CSS `aspect-ratio`) to reserve space.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["a11y", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
