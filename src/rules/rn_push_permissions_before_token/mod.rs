//! rn-push-permissions-before-token — require permission request before push token.
//!
//! Calling `getExpoPushTokenAsync` before checking notification permissions
//! surfaces a confusing dialog sequence on iOS and fails silently on Android.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-push-permissions-before-token",
    description: "`getExpoPushTokenAsync` must be preceded by `requestPermissionsAsync` in the same function.",
    remediation: "Await `Notifications.requestPermissionsAsync()` before requesting the push token.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
