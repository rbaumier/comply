//! tailwind-require-aspect-ratio-on-media — flag `<img>` / `<video>`
//! elements lacking `aspect-*` Tailwind classes AND `width` + `height`
//! attributes. Without an aspect ratio, the browser cannot reserve
//! space, causing CLS.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-require-aspect-ratio-on-media",
    description: "`<img>` / `<video>` without `aspect-*` or width+height causes layout shift.",
    remediation: "Add a Tailwind `aspect-*` class (e.g. `aspect-video`) or both `width` and `height` attributes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
