//! react-no-deprecated — flag usage of deprecated React / ReactDOM APIs
//! and legacy class lifecycle methods.
//!
//! Why: React has signalled the eventual removal of these APIs since
//! React 16.3. Keeping them in the codebase blocks upgrades to concurrent
//! rendering (`createRoot`, `hydrateRoot`) and hides subtle bugs in the
//! legacy lifecycle methods that fire inconsistently under Strict Mode.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-deprecated",
    description: "Deprecated React APIs should not be used.",
    remediation: "Replace the deprecated API with its modern equivalent.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-deprecated.md",
    ),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
