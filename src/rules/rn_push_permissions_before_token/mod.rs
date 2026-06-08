//! rn-push-permissions-before-token — require permission request before push token.
//!
//! Calling `getExpoPushTokenAsync` before checking notification permissions
//! surfaces a confusing dialog sequence on iOS and fails silently on Android.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-push-permissions-before-token",
    description: "`getExpoPushTokenAsync` must be preceded by `requestPermissionsAsync` in the same function.",
    remediation: "Await `Notifications.requestPermissionsAsync()` before requesting the push token.",
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
