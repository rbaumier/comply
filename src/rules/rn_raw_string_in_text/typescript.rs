//! Flags non-whitespace `jsx_text` and number/string `jsx_expression` children
//! inside a JSX element whose tag is NOT `Text` (or a custom Text-like tag).

use crate::diagnostic::{Diagnostic, Severity};

// RN components that are text containers by convention. Anything not in this
// set triggers the warning when it has a string/number child.
fn is_text_host(tag: &str) -> bool {
    // Accept `Text`, `HeaderText`, `RegularText`… anything ending in `Text`,
    // plus explicit Expo/RN text-typographic components.
    tag.ends_with("Text") || tag == "Heading" || tag == "Label"
}

// RN components we KNOW are layout containers and therefore cannot hold text.
// We only warn for these to avoid flagging plain HTML `<div>` in non-RN TSX.
fn is_rn_container(tag: &str) -> bool {
    matches!(
        tag,
        "View"
            | "ScrollView"
            | "SafeAreaView"
            | "KeyboardAvoidingView"
            | "Pressable"
            | "TouchableOpacity"
            | "TouchableHighlight"
            | "TouchableWithoutFeedback"
    )
}

crate::ast_check! { on ["jsx_element"] => |node, source, ctx, diagnostics|
    let Some(opening) = node.child(0) else { return };
    if opening.kind() != "jsx_opening_element" { return; }
    let Some(tag_node) = opening.child_by_field_name("name") else { return };
    let Ok(tag) = tag_node.utf8_text(source) else { return };
    if is_text_host(tag) { return; }
    if !is_rn_container(tag) { return; }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "jsx_text" => {
                let Ok(text) = child.utf8_text(source) else { continue };
                if text.trim().is_empty() { continue; }
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &child,
                    super::META.id,
                    format!("Raw string child in `<{tag}>` — wrap in `<Text>` to avoid a runtime error."),
                    Severity::Warning,
                ));
            }
            "jsx_expression" => {
                // Check if the inner expression is a string/number literal.
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    match inner.kind() {
                        "string" | "number" | "template_string" => {
                            diagnostics.push(Diagnostic::at_node(
                                ctx.path,
                                &child,
                                super::META.id,
                                format!("String/number expression child in `<{tag}>` — wrap in `<Text>`."),
                                Severity::Warning,
                            ));
                            break;
                        }
                        "{" | "}" => continue,
                        _ => break,
                    }
                }
            }
            _ => {}
        }
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_raw_string_in_view() {
        let src = "const x = <View>hello</View>;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_number_expression_in_view() {
        let src = "const x = <View>{42}</View>;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_string_inside_text() {
        let src = "const x = <Text>hello</Text>;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_nested_text_in_view() {
        let src = "const x = <View><Text>hello</Text></View>;";
        assert!(run(src).is_empty());
    }
}
