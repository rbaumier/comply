//! rn-biometrics-hardware-check — check hardware/enrolment before `authenticateAsync`.
//!
//! Calling `authenticateAsync` on a device without biometric hardware (or
//! without any enrolled biometric) returns an unhelpful failure. Gate the
//! call on `hasHardwareAsync` and `isEnrolledAsync`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-biometrics-hardware-check",
    description: "`authenticateAsync` must be preceded by `hasHardwareAsync` / `isEnrolledAsync`.",
    remediation: "Await both checks before calling `LocalAuthentication.authenticateAsync()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
