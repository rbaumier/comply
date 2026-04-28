//! ui-no-scale-from-zero — flag inline `transform: scale(0)` styles.
//! Scaling from zero causes subpixel rendering blur during the animation
//! and makes elements appear from nowhere; prefer `scale(0.95)` paired
//! with `opacity: 0` for a natural entrance.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-scale-from-zero",
    description: "Inline `transform: scale(0)` causes subpixel rendering blur and makes elements \
                  appear from nowhere.",
    remediation: "Use `scale(0.95)` with `opacity: 0` for a smoother, more natural entrance.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
