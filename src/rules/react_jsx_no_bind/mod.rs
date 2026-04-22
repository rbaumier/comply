//! react-jsx-no-bind — disallow `.bind()` and arrow functions as JSX prop values.
//!
//! Creating a new function reference on every render (via `.bind(this)` or
//! `onClick={() => ...}`) breaks referential equality, defeats `React.memo`
//! / `PureComponent` optimizations, and forces child components to re-render.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-bind",
    description: "Arrow functions and `.bind()` in JSX props create a new reference every render.",
    remediation: "Hoist the handler to a stable reference — `useCallback`, a class method, \
                  or a module-level function — so memoized children don't re-render needlessly.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/jsx-no-bind.md",
    ),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
