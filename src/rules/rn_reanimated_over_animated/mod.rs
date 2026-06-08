//! rn-reanimated-over-animated — prefer `react-native-reanimated` over legacy `Animated`.
//!
//! The legacy `Animated` API runs on the JS thread and drops frames under
//! load. `react-native-reanimated` runs animations on the UI thread.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-reanimated-over-animated",
    description: "Legacy `Animated` from react-native runs on the JS thread and drops frames.",
    remediation: "Use `react-native-reanimated` primitives (`useSharedValue`, `withTiming`).",
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
