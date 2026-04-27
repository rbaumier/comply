//! next-no-api-route-in-middleware

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-api-route-in-middleware",
    description: "Calling your own API route from middleware causes a same-origin fetch loop.",
    remediation: "Inline the logic, call a shared helper, or invoke a third-party endpoint — never fetch your own `/api/*` from middleware.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/building-your-application/routing/middleware"),
    categories: &["nextjs", "reliability"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
