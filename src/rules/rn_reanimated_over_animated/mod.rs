//! rn-reanimated-over-animated — prefer `react-native-reanimated` over legacy `Animated`.
//!
//! The legacy `Animated` API runs on the JS thread and drops frames under
//! load. `react-native-reanimated` runs animations on the UI thread.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-reanimated-over-animated",
    description: "Legacy `Animated` from react-native runs on the JS thread and drops frames.",
    remediation: "Use `react-native-reanimated` primitives (`useSharedValue`, `withTiming`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
