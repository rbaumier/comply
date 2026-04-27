//! next-no-img-element

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-img-element",
    description: "Using `<img>` instead of `next/image` disables image optimization.",
    remediation: "Replace `<img>` with `<Image>` from `next/image` to enable lazy loading and automatic resizing.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/messages/no-img-element"),
    categories: &["nextjs", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
