//! react-hook-form-validation-mode — `useForm` must set `mode: "onTouched"`
//! and `reValidateMode: "onChange"`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-hook-form-validation-mode",
    description: "React Hook Form's `useForm` must validate on blur and re-validate on change: \
                  `mode: \"onTouched\"` shows errors only after a field is touched, and \
                  `reValidateMode: \"onChange\"` clears them as the user fixes them.",
    remediation: "Pass `{ mode: \"onTouched\", reValidateMode: \"onChange\" }` to `useForm`.",
    severity: Severity::Warning,
    doc_url: Some("https://react-hook-form.com/docs/useform"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
