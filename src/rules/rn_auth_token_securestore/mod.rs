//! rn-auth-token-securestore — ban storing auth tokens in AsyncStorage.
//!
//! AsyncStorage is unencrypted; auth tokens belong in `expo-secure-store`
//! which uses the platform Keychain / Keystore.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-auth-token-securestore",
    description: "Auth tokens must not be written to AsyncStorage (unencrypted).",
    remediation: "Use `expo-secure-store` (`SecureStore.setItemAsync`) for tokens.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native", "security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
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
