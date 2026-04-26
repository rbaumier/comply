//! react-jsx-no-new-array-as-prop — disallow array literals as JSX prop values.
//!
//! An array literal written directly inside a JSX prop (`items={[1, 2, 3]}`)
//! allocates a new array on every render. That new reference breaks
//! `React.memo` / `PureComponent` equality checks and forces the child
//! component to re-render even when the contents are identical.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-new-array-as-prop",
    description: "Array literals as JSX prop values create a new reference every render.",
    remediation: "Extract array to a constant or use useMemo",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
