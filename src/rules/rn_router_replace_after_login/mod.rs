//! rn-router-replace-after-login — require `router.replace` after auth transitions.
//!
//! After login/logout, the previous screen should not remain on the back
//! stack. `router.push` keeps it there; `router.replace` discards it.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-router-replace-after-login",
    description: "Navigating after login/logout must not keep the previous screen on the back stack.",
    remediation: "Use `router.replace('/path')` instead of `router.push('/path')` after auth.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
