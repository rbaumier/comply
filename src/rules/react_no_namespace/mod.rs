//! react-no-namespace — namespaced JSX elements (e.g. `<Foo:bar>`).

mod vue;
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-namespace",
    description: "Namespaced JSX elements (`<Foo:bar>`) are not supported by React.",
    remediation: "React does not support XML namespaces in JSX. Use a different \
                  naming pattern (e.g., `FooBar` or `Foo.Bar`).",
    severity: Severity::Error,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-namespace.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, react).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(vue::Check))));
    RuleDef { meta: META, backends }
}
