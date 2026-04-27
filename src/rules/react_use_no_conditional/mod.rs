//! react-use-no-conditional — React 19's `use(promise)` hook must follow the
//! same rules as other hooks: it cannot be called conditionally or inside a
//! loop.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-use-no-conditional",
    description: "`use(...)` (React 19) must not be called conditionally — same rules as other hooks.",
    remediation: "Move the `use(...)` call to the top of the component, then conditionally use the value. \
                  If the value isn't always needed, restructure with separate components.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/react/use"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
