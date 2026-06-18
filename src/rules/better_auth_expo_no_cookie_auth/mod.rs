mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-expo-no-cookie-auth",
    description: "React Native/Expo apps must use `expoClient()` from `@better-auth/expo`.",
    remediation: "Import `expoClient` from `@better-auth/expo/client` and pass it via `plugins` to `createAuthClient`.",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/integrations/expo"),
    categories: &["better-auth", "react-native"],

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
