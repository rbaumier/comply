//! rn-raw-string-in-text OXC backend — flag raw strings/numbers inside RN container JSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName, JSXExpression};
use std::sync::Arc;

pub struct Check;

fn is_text_host(tag: &str) -> bool {
    tag.ends_with("Text") || tag == "Heading" || tag == "Label"
}

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

fn jsx_element_name_str<'a>(name: &'a JSXElementName<'a>) -> Option<&'a str> {
    match name {
        JSXElementName::Identifier(id) => Some(id.name.as_str()),
        JSXElementName::IdentifierReference(id) => Some(id.name.as_str()),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        let Some(tag) = jsx_element_name_str(&opening.name) else {
            return;
        };
        if is_text_host(tag) {
            return;
        }
        if !is_rn_container(tag) {
            return;
        }

        // Walk up to find the parent JSXElement, then check its children
        let parent = semantic.nodes().parent_node(node.id());
        let AstKind::JSXElement(element) = parent.kind() else {
            return;
        };

        for child in &element.children {
            match child {
                JSXChild::Text(text) => {
                    if text.value.trim().is_empty() {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, text.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Raw string child in `<{tag}>` \u{2014} wrap in `<Text>` to avoid a runtime error."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                JSXChild::ExpressionContainer(container) => {
                    let is_string_or_number = matches!(
                        &container.expression,
                        JSXExpression::StringLiteral(_)
                            | JSXExpression::NumericLiteral(_)
                            | JSXExpression::TemplateLiteral(_)
                    );
                    if is_string_or_number {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, container.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "String/number expression child in `<{tag}>` \u{2014} wrap in `<Text>`."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
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
