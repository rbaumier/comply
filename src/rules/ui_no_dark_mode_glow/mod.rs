//! ui-no-dark-mode-glow — colored box-shadow glow on a dark background looks
//! cheap and reduces contrast.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-dark-mode-glow",
    description: "Colored box-shadow glow on a dark background — prefer subtle neutral shadows.",
    remediation: "Use a neutral shadow (e.g. `rgba(0,0,0,0.3)`) or a very \
                  subtle tinted shadow instead of a bright glow effect.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
