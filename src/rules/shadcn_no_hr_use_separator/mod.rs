//! shadcn-no-hr-use-separator — forbid raw `<hr>` in JSX; require the
//! shadcn `<Separator />` component so theming and a11y stay uniform.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-hr-use-separator",
    description: "Raw `<hr>` bypasses shadcn theming — use the `<Separator />` component.",
    remediation: "Replace `<hr />` with `<Separator />` (or `<Separator orientation=\"vertical\" />`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn"],

    skip_in_test_dir: false,
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
