//! react-no-namespace — namespaced JSX elements (e.g. `<Foo:bar>`).

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
    let backends = vec![
        (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
    ];
    RuleDef {
        meta: META,
        backends,
    }
}
