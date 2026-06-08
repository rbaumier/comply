//! shadcn-no-space-x-y — forbid `space-x-*` / `space-y-*` utilities in
//! JSX `className`; prefer `flex` + `gap-*`, which plays nicely with
//! shadcn layout primitives and RTL.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-space-x-y",
    description: "`space-x-*` / `space-y-*` produce brittle layouts — use flex/grid + gap-* instead.",
    remediation: "Replace `space-x-2` with `flex gap-2` and `space-y-4` with `flex flex-col gap-4`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn", "tailwind"],

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
