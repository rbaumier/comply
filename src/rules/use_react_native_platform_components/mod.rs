//! use-react-native-platform-components — keep platform-specific React Native
//! components in platform-specific files.
//!
//! React Native resolves `Foo.android.js` / `Foo.ios.js` per platform. A
//! component imported from `react-native` whose name ends in a platform marker
//! (`*Android`, `*IOS`) only ships correct code when it lives in a file of the
//! matching platform; importing it into a shared file pulls the wrong bundle.
//! Mixing an `*Android` and an `*IOS` component in one file is always wrong.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "use-react-native-platform-components",
    description: "Platform-specific React Native components belong in platform-specific files.",
    remediation: "Move the import to a file with the matching platform suffix (`.android.*` / `.ios.*`), or split mixed platforms into separate files.",
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
