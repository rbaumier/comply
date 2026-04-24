//! perf-img-modern-format — flag `<img>` with legacy raster formats when not
//! wrapped in a `<picture>` element offering WebP/AVIF alternatives.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "perf-img-modern-format",
    description: "`<img>` with .jpg/.jpeg/.png should provide a WebP/AVIF fallback via `<picture>` or `srcset`.",
    remediation: "Wrap the image in a `<picture>` with `<source type=\"image/webp\">`, or use `srcset` with a modern format.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["web-performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
