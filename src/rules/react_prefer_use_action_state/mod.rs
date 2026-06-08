//! react-prefer-use-action-state — a file that combines `useState` +
//! `useTransition` + a `<form action={}>` is reimplementing the work
//! `useActionState` does in one hook.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-prefer-use-action-state",
    description: "Manual `useState` + `useTransition` + form action is reinventing `useActionState`.",
    remediation: "Replace the trio with `const [state, dispatch, pending] = useActionState(action, initial);`.",
    severity: Severity::Warning,
    doc_url: Some("https://react.dev/reference/react/useActionState"),
    categories: &["react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
