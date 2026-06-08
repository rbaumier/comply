//! rn-push-token-requires-projectid — `getExpoPushTokenAsync` must pass `{ projectId }`.
//!
//! Without `projectId`, EAS cannot route notifications to the right project,
//! which silently breaks push delivery in production builds.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-push-token-requires-projectid",
    description: "`getExpoPushTokenAsync` must be called with `{ projectId }`.",
    remediation: "Pass `{ projectId: Constants.expoConfig.extra.eas.projectId }`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
