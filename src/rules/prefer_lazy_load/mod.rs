//! prefer-lazy-load
//!
//! Flags `<img>` and `<iframe>` elements without `loading="lazy"`.
//! Native lazy loading defers off-screen image/iframe fetches until
//! they approach the viewport, reducing initial page weight and
//! improving LCP. Elements explicitly marked `loading="eager"` are
//! allowed (intentional opt-out, e.g. hero images).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-lazy-load",
    description: "`<img>` and `<iframe>` should set `loading=\"lazy\"` to defer off-screen loads.",
    remediation: "Add `loading=\"lazy\"` (or `loading=\"eager\"` for above-the-fold assets).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],
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
