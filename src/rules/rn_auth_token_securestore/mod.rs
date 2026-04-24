//! rn-auth-token-securestore — ban storing auth tokens in AsyncStorage.
//!
//! AsyncStorage is unencrypted; auth tokens belong in `expo-secure-store`
//! which uses the platform Keychain / Keystore.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-auth-token-securestore",
    description: "Auth tokens must not be written to AsyncStorage (unencrypted).",
    remediation: "Use `expo-secure-store` (`SecureStore.setItemAsync`) for tokens.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native", "security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
