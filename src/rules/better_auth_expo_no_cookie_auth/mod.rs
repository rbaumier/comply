mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-expo-no-cookie-auth",
    description: "React Native/Expo apps must use `expoClient()` from `@better-auth/expo`.",
    remediation: "Import `expoClient` from `@better-auth/expo/client` and pass it via `plugins` to `createAuthClient`.",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/integrations/expo"),
    categories: &["better-auth", "react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
