//! react-hoist-static-jsx — hoist static JSX above the component body.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-hoist-static-jsx",
    description: "JSX with no dynamic content defined inside a component is \
                  rebuilt every render.",
    remediation: "Assign the static JSX to a module-level `const` above the \
                  component (or `React.memo` it). Re-creating an identical \
                  element tree on every render wastes reconciler work and \
                  prevents `shouldComponentUpdate`/`React.memo` short-circuits \
                  in consumers.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
