//! next-prefer-next-env

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-prefer-next-env",
    description: "Reading `window.__NEXT_DATA__` or `__NEXT_DATA__` is brittle and unsupported.",
    remediation: "Read configuration via `process.env.NEXT_PUBLIC_*` (build-time inlined) instead of the legacy `__NEXT_DATA__` global.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/app/building-your-application/configuring/environment-variables"),
    categories: &["nextjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
