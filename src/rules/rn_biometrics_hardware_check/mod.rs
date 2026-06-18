//! rn-biometrics-hardware-check — check hardware/enrolment before `authenticateAsync`.
//!
//! Calling `authenticateAsync` on a device without biometric hardware (or
//! without any enrolled biometric) returns an unhelpful failure. Gate the
//! call on `hasHardwareAsync` and `isEnrolledAsync`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-biometrics-hardware-check",
    description: "`authenticateAsync` must be preceded by `hasHardwareAsync` / `isEnrolledAsync`.",
    remediation: "Await both checks before calling `LocalAuthentication.authenticateAsync()`.",
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
