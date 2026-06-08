//! rn-no-react-navigation-stack — ban `@react-navigation/stack` in favour of Expo Router.
//!
//! Expo Router supersedes the legacy stack navigator. Mixing the two leads to
//! duplicated navigation state and type-unsafe route references.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-no-react-navigation-stack",
    description: "`@react-navigation/stack` and `createStackNavigator` are forbidden; use Expo Router.",
    remediation: "Delete the stack navigator and migrate routes to Expo Router file-based routing.",
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
