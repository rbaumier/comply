//! a11y-interactive-supports-focus

mod oxc_typescript;
#[cfg(test)]
mod react;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-interactive-supports-focus",
    description: "Elements with interactive handlers and an interactive role must be focusable.",
    remediation: "Add `tabIndex={0}` to elements that have `onClick`/`onKeyDown` and an interactive `role`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    let mut backends = vec![
        (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
    ];
    backends.push((Language::Vue, Backend::Text(Box::new(vue::Check))));
    RuleDef {
        meta: META,
        backends,
    }
}
