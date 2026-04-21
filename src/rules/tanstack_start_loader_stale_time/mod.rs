//! tanstack-start-loader-stale-time

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-loader-stale-time",
    description: "Loader `staleTime` too short — data will refetch during navigation.",
    remediation: "Set `staleTime: 5000` or more (ms) on `ensureQueryData` loader calls to cover navigation duration.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack", "performance"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
