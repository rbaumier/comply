//! tanstack-start-loader-stale-time

mod typescript;

use crate::diagnostic::Severity;
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
    crate::register_ts_family!(META, typescript)
}
