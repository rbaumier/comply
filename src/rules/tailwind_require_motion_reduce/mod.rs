//! tailwind-require-motion-reduce — require `motion-reduce:*` on any
//! element that uses a `transition-*` or `animate-*` utility, so users
//! who set `prefers-reduced-motion: reduce` aren't forced to watch
//! animations they opted out of.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-require-motion-reduce",
    description: "Elements with `transition-*` / `animate-*` must also declare a `motion-reduce:*` variant.",
    remediation: "Add `motion-reduce:transition-none` (or `motion-reduce:animate-none`) so users with `prefers-reduced-motion: reduce` are respected.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
