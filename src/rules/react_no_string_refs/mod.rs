//! react-no-string-refs — string `ref` attributes in JSX.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-string-refs",
    description: "String `ref` attributes are deprecated — use `useRef` / callback refs.",
    remediation: "Replace `ref=\"myRef\"` with a `useRef()` hook or a callback ref. \
                  String refs are a legacy API that has been removed in React 19.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-string-refs.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
