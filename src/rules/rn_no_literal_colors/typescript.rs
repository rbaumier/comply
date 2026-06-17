//! Regression tests recovering Biome's `noReactNativeLiteralColors` fixtures.

use super::oxc_typescript::Check;
use crate::diagnostic::Diagnostic;

/// Runs the rule for a React Native project (the framework gate is satisfied).
fn run(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule_with_ctx(
        &Check,
        src,
        "t.tsx",
        &crate::project::ProjectCtx::for_test_with_framework("react-native"),
        crate::rules::file_ctx::default_static_file_ctx(),
    )
}

// ── invalid.jsx — each color literal in an RN style context fires ──────────

#[test]
fn flags_inline_background_color() {
    let src =
        "const Inline = () => <Text style={{ backgroundColor: '#FFFFFF', opacity: 0.5 }}>hello</Text>;";
    assert_eq!(run(src).len(), 1);
}

#[test]
fn flags_font_color_in_stylesheet_create() {
    let src = "const stylesBasic = StyleSheet.create({\n\ttext: { fontColor: '#000' },\n});";
    assert_eq!(run(src).len(), 1);
}

#[test]
fn flags_multiple_colors_in_stylesheet_create() {
    let src = "const MultipleInSheet = StyleSheet.create({\n\
                \tprimary: { color: 'red' },\n\
                \tsecondary: { borderBottomColor: 'blue' },\n\
                });";
    assert_eq!(run(src).len(), 2);
}

#[test]
fn flags_color_in_style_array_object() {
    let src = "const InArray = () => (\n\
                \t<Text style={[styles.text, { backgroundColor: '#FFFFFF' }]}>hello</Text>\n\
                );";
    assert_eq!(run(src).len(), 1);
}

#[test]
fn flags_color_in_logical_style_object() {
    let src = "const InLogical = ({ active }) => (\n\
                \t<Text style={[styles.text, active && { backgroundColor: '#FFFFFF' }]}>hello</Text>\n\
                );";
    assert_eq!(run(src).len(), 1);
}

#[test]
fn flags_ternary_both_string_literals() {
    let src = "const T = ({ active }) => (\n\
                \t<Text style={{ backgroundColor: active ? '#fff' : '#000' }}>hello</Text>\n\
                );";
    assert_eq!(run(src).len(), 1);
}

#[test]
fn flags_ternary_one_string_literal() {
    let src = "const T = ({ active }) => (\n\
                \t<Text style={{ backgroundColor: active ? '#fff' : theme.background }}>hello</Text>\n\
                );";
    assert_eq!(run(src).len(), 1);
}

#[test]
fn flags_custom_style_attribute() {
    let src = "const C = () => (\n\
                \t<Text contentContainerStyle={{ color: 'red' }}>hello</Text>\n\
                );";
    assert_eq!(run(src).len(), 1);
}

// ── invalidReactNativeImport.jsx — imported from react-native fires ────────

#[test]
fn flags_stylesheet_create_imported_from_react_native() {
    let src = "import { StyleSheet } from 'react-native';\n\
                const styles = StyleSheet.create({\n\
                \ttext: { color: 'red' },\n\
                });";
    assert_eq!(run(src).len(), 1);
}

// ── valid.jsx — colors referenced via variables / non-color props ──────────

#[test]
fn allows_color_from_variable_in_stylesheet_create() {
    let src = "const red = '#f00';\n\
                const blue = '#00f';\n\
                const stylesFromVars = StyleSheet.create({\n\
                \ttitle: { color: red },\n\
                \tsubtitle: { color: blue },\n\
                });";
    assert!(run(src).is_empty());
}

#[test]
fn allows_themed_color_member() {
    let src = "const Themed = () => <Text style={{ color: theme.primary }}>hello</Text>;";
    assert!(run(src).is_empty());
}

#[test]
fn allows_ternary_of_variables() {
    let src = "const C = ({ isDanger }) => {\n\
                \tconst trueColor = '#fff';\n\
                \tconst falseColor = '#000';\n\
                \treturn (\n\
                \t\t<View style={[{ color: isDanger ? trueColor : falseColor }, isDanger && { color: trueColor }]} />\n\
                \t);\n\
                };";
    assert!(run(src).is_empty());
}

#[test]
fn allows_non_color_string_literal() {
    let src = "const N = StyleSheet.create({\n\
                \tbox: { fontFamily: 'Arial', padding: 10 },\n\
                });";
    assert!(run(src).is_empty());
}

#[test]
fn allows_shorthand_color_property() {
    let src = "const S = ({ color }) => (\n\t<Text style={{ color }}>hello</Text>\n);";
    assert!(run(src).is_empty());
}

// ── Over-firing guards — color strings OUTSIDE the RN style context ────────

#[test]
fn allows_color_literal_in_plain_object_outside_style() {
    // Not a style prop, not StyleSheet.create — a free object literal.
    let src = "const OutsideStyleContext = {\n\tbackgroundColor: '#fff',\n};";
    assert!(run(src).is_empty());
}

#[test]
fn allows_color_literal_returned_from_plain_function() {
    let src = "function paintBackground() {\n\treturn { backgroundColor: '#fff' };\n}";
    assert!(run(src).is_empty());
}

#[test]
fn allows_create_on_non_stylesheet_object() {
    let src = "const NonStyleSheetCreate = MyThing.create({\n\
                \tbox: { backgroundColor: '#fff' },\n\
                });";
    assert!(run(src).is_empty());
}

#[test]
fn allows_color_literal_in_non_style_jsx_attribute() {
    let src = "const N = () => (\n\t<View data={{ backgroundColor: '#fff' }}>hello</View>\n);";
    assert!(run(src).is_empty());
}

// ── Framework gate — web React DOM `style={{...}}` must not fire (#3998) ───

#[test]
fn ignores_dom_inline_style_on_web_project() {
    // Plain web React (no `react-native` framework): a DOM `<div style={{...}}>`
    // with a color literal is normal CSS, not a React Native style.
    let src = "const ButtonSeparator = () => <div style={{ backgroundColor: '#fff' }} />;";
    assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
}

#[test]
fn flags_dom_inline_style_on_react_native_project() {
    // Same shape, but on a React Native project — the rule fires.
    let src = "const ButtonSeparator = () => <div style={{ backgroundColor: '#fff' }} />;";
    assert_eq!(run(src).len(), 1);
}

// ── validCustomStyleSheet.jsx — StyleSheet from another package ────────────

#[test]
fn allows_stylesheet_create_imported_from_other_package() {
    let src = "import { StyleSheet } from 'my-custom-lib';\n\
                const FromOtherPackage = StyleSheet.create({\n\
                \tbox: { backgroundColor: '#fff' },\n\
                });";
    assert!(run(src).is_empty());
}

// ── validLocalStyleSheet.jsx — locally-declared StyleSheet ─────────────────

#[test]
fn allows_create_on_local_stylesheet_binding() {
    let src = "const StyleSheet = { create: (value) => value };\n\
                const LocalSheet = StyleSheet.create({\n\
                \tbox: { backgroundColor: '#fff' },\n\
                });";
    assert!(run(src).is_empty());
}
