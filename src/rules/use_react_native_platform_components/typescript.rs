//! Regression tests recovering Biome's `useReactNativePlatformComponents`
//! fixtures. The rule is path-sensitive: the test path's platform suffix
//! (`.android.js` / `.ios.js`) decides which platform components are allowed.

use super::oxc_typescript::Check;
use crate::diagnostic::Diagnostic;

fn run(src: &str, path: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&Check, src, path)
}

// ── invalid.js — platform components in a shared (non-platform) file ────────

#[test]
fn flags_android_component_in_shared_file() {
    let src = "import { ProgressBarAndroid } from \"react-native\";";
    assert_eq!(run(src, "shared.js").len(), 1);
}

#[test]
fn flags_ios_component_in_shared_file() {
    let src = "import { ActivityIndicatorIOS } from \"react-native\";";
    assert_eq!(run(src, "shared.js").len(), 1);
}

#[test]
fn flags_mixed_ios_and_android_aliased_import() {
    // `as Foo` alias does not change the source-name classification.
    let src = "import { ActivityIndicatorIOS as Foo, ProgressBarAndroid } from \"react-native\";";
    let diags = run(src, "shared.js");
    assert_eq!(diags.len(), 2);
    assert!(diags.iter().all(|d| d.message.contains("cannot be mixed")));
}

#[test]
fn flags_android_require_destructuring() {
    let src = "const { ProgressBarAndroid: Bar } = require(\"react-native\");";
    assert_eq!(run(src, "shared.js").len(), 1);
}

#[test]
fn flags_ios_require_destructuring() {
    let src = "const { ActivityIndicatorIOS: Baz } = require(\"react-native\");";
    assert_eq!(run(src, "shared.js").len(), 1);
}

// ── valid.js — platform-agnostic components in a shared file ────────────────

#[test]
fn allows_agnostic_named_imports() {
    let src = "import { View } from \"react-native\";\n\
               import { Text, ScrollView } from \"react-native\";";
    assert!(run(src, "shared.js").is_empty());
}

#[test]
fn allows_agnostic_require_destructuring() {
    let src = "const { View: MyView } = require(\"react-native\");";
    assert!(run(src, "shared.js").is_empty());
}

// ── valid.android.js / valid.ios.js — platform file allows its own ──────────

#[test]
fn allows_android_component_in_android_file() {
    let src = "import { ProgressBarAndroid } from \"react-native\";\n\
               const { ProgressBarAndroid: Bar } = require(\"react-native\");";
    assert!(run(src, "valid.android.js").is_empty());
}

#[test]
fn allows_ios_component_in_ios_file() {
    let src = "import { ActivityIndicatorIOS } from \"react-native\";\n\
               const { ActivityIndicatorIOS: Baz } = require(\"react-native\");";
    assert!(run(src, "valid.ios.js").is_empty());
}

#[test]
fn flags_ios_component_in_android_file() {
    // A platform file still rejects the *other* platform's components.
    let src = "import { ActivityIndicatorIOS } from \"react-native\";";
    let diags = run(src, "valid.android.js");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("iOS component"));
}

#[test]
fn flags_android_component_in_ios_file() {
    let src = "import { ProgressBarAndroid } from \"react-native\";";
    let diags = run(src, "valid.ios.js");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("Android component"));
}

// ── Diagnostic message wording for a single-platform mismatch ───────────────

#[test]
fn android_mismatch_message_names_the_component() {
    let src = "import { ProgressBarAndroid } from \"react-native\";";
    let diags = run(src, "shared.js");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("Android component `ProgressBarAndroid`"));
    assert!(!diags[0].message.contains("cannot be mixed"));
}

// ── Over-firing guards ─────────────────────────────────────────────────────

#[test]
fn ignores_platform_named_import_from_other_package() {
    // The marker matters only for imports from `react-native`.
    let src = "import { ProgressBarAndroid } from \"some-other-lib\";";
    assert!(run(src, "shared.js").is_empty());
}

#[test]
fn ignores_require_from_other_package() {
    let src = "const { ProgressBarAndroid } = require(\"some-other-lib\");";
    assert!(run(src, "shared.js").is_empty());
}

#[test]
fn ignores_type_only_import() {
    let src = "import type { ProgressBarAndroid } from \"react-native\";";
    assert!(run(src, "shared.ts").is_empty());
}

#[test]
fn ignores_inline_type_specifier() {
    let src = "import { type ProgressBarAndroid } from \"react-native\";";
    assert!(run(src, "shared.ts").is_empty());
}

#[test]
fn ignores_nested_directory_path_for_platform_suffix() {
    // `**/` in the default glob matches a nested platform file.
    let src = "import { ProgressBarAndroid } from \"react-native\";";
    assert!(run(src, "app/components/list.android.tsx").is_empty());
}
