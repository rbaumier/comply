//! tailwind-min-touch-target — flag interactive elements whose computed
//! Tailwind size falls below the ~44x44px target WCAG AAA recommends
//! (2.5.5 Target Size). Heuristic: button / a / role=button with tiny
//! padding (`px-1`, `py-0`, `p-1`) and no explicit `h-*` / `w-*`
//! overriding the size gets flagged.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-min-touch-target",
    description: "Interactive elements should be ~44x44px minimum (WCAG 2.5.5).",
    remediation: "Bump padding / height so the touch target reaches 44px (e.g. `h-11 px-4`, or `min-h-11 min-w-11` for icon buttons).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
