//! tailwind-require-aspect-ratio-on-media — flag `<img>` / `<video>`
//! elements lacking `aspect-*` Tailwind classes AND `width` + `height`
//! attributes. Without an aspect ratio, the browser cannot reserve
//! space, causing CLS.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-require-aspect-ratio-on-media",
    description: "`<img>` / `<video>` without `aspect-*` or width+height causes layout shift.",
    remediation: "Add a Tailwind `aspect-*` class (e.g. `aspect-video`) or both `width` and `height` attributes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "performance"],

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
