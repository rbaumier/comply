//! react-require-content-visibility — large `.map()` lists rendered without virtualization.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-require-content-visibility",
    description: "A `.map()` in JSX producing 20+ items with no virtualization wrapper \
                  and no `content-visibility: auto` hint paints every off-screen item.",
    remediation: "Wrap the list in a virtualizer (`react-window`, `react-virtuoso`) \
                  or set `style={{ contentVisibility: 'auto' }}` on each row.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "web-performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
