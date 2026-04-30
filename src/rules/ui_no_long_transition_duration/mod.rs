//! ui-no-long-transition-duration — inline transition/animation durations
//! above 1s feel sluggish and block user interaction.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-long-transition-duration",
    description: "Inline `transitionDuration`/`animationDuration` above 1s — feels sluggish.",
    remediation: "Keep UI transitions under 1s (typically 150-400ms). Long durations block \
                  interaction and harm perceived performance.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
