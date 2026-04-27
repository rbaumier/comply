//! next-no-head-element

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-head-element",
    description: "Using a raw `<head>` element bypasses Next.js head management.",
    remediation: "Use the `Head` component from `next/head` (pages router) or the metadata API (app router) instead of `<head>`.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/messages/no-head-element"),
    categories: &["nextjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
