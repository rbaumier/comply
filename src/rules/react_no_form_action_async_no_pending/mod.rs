//! react-no-form-action-async-no-pending — `<form action={asyncFn}>` without
//! a way to surface the pending state leaves the form unresponsive during
//! submission. Use `useFormStatus`, `useActionState`, or `useTransition`.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-form-action-async-no-pending",
    description: "`<form action={...}>` is used without a pending-state hook — submitters get no feedback.",
    remediation: "Read the pending state via `useFormStatus()` inside a child of the form, \
                  or switch to `useActionState`. For non-form actions, use `useTransition`.",
    severity: Severity::Warning,
    doc_url: Some("https://react.dev/reference/react-dom/hooks/useFormStatus"),
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
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
