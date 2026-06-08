//! tailwind-min-touch-target — flag interactive elements whose computed
//! Tailwind size falls below the ~44x44px target WCAG AAA recommends
//! (2.5.5 Target Size). Heuristic: button / a / role=button with tiny
//! padding (`px-1`, `py-0`, `p-1`) and no explicit `h-*` / `w-*`
//! overriding the size gets flagged.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-min-touch-target",
    description: "Interactive elements should be ~44x44px minimum (WCAG 2.5.5).",
    remediation: "Bump padding / height so the touch target reaches 44px (e.g. `h-11 px-4`, or `min-h-11 min-w-11` for icon buttons).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "a11y"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
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
