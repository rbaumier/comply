//! react-no-namespace — namespaced JSX elements (e.g. `<Foo:bar>`).

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-namespace",
    description: "Namespaced JSX elements (`<Foo:bar>`) are not supported by React.",
    remediation: "React does not support XML namespaces in JSX. Use a different \
                  naming pattern (e.g., `FooBar` or `Foo.Bar`).",
    severity: Severity::Error,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-namespace.md",
    ),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let backends = crate::register_ts_family!(META, react).backends;
    RuleDef {
        meta: META,
        backends,
    }
}
