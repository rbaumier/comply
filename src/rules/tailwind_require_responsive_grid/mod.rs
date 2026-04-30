//! tailwind-require-responsive-grid — a `grid-cols-2`+ without a mobile
//! fallback compresses into unreadable slivers on phones. Require either
//! `grid-cols-1` as the base and the multi-column count behind a
//! breakpoint, or an explicit mobile-first pair.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-require-responsive-grid",
    description: "`grid-cols-2+` without a responsive variant compresses on mobile.",
    remediation: "Use `grid-cols-1 md:grid-cols-3` (mobile-first) instead of `grid-cols-3`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
