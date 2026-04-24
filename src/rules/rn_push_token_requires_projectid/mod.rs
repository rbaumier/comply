//! rn-push-token-requires-projectid — `getExpoPushTokenAsync` must pass `{ projectId }`.
//!
//! Without `projectId`, EAS cannot route notifications to the right project,
//! which silently breaks push delivery in production builds.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-push-token-requires-projectid",
    description: "`getExpoPushTokenAsync` must be called with `{ projectId }`.",
    remediation: "Pass `{ projectId: Constants.expoConfig.extra.eas.projectId }`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
