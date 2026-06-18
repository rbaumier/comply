//! perf-img-modern-format — flag `<img>` with legacy raster formats when not
//! wrapped in a `<picture>` element offering WebP/AVIF alternatives.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "perf-img-modern-format",
    description: "`<img>` with .jpg/.jpeg/.png should provide a WebP/AVIF fallback via `<picture>` or `srcset`.",
    remediation: "Wrap the image in a `<picture>` with `<source type=\"image/webp\">`, or use `srcset` with a modern format.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["web-performance"],

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
