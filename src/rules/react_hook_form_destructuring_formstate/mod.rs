//! react-hook-form-destructuring-formstate — require destructuring `formState`
//! instead of accessing fields via `formState.xxx`.
//!
//! React Hook Form tracks which `formState` properties are actually read so it
//! can skip re-renders when other fields change. That proxy tracking only
//! works at *destructuring time*: `const { isValid } = formState` subscribes
//! only to `isValid`, while `formState.isValid` forces a subscription to the
//! whole object and re-renders on every field change.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-hook-form-destructuring-formstate",
    description: "Accessing `formState.xxx` without destructuring defeats React Hook Form proxy tracking.",
    remediation: "Destructure the needed fields up front: `const { isValid, errors } = formState;`.",
    severity: Severity::Warning,
    doc_url: Some("https://react-hook-form.com/docs/useform/formstate"),
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
