//! rn-reanimated-over-animated oxc backend — flag Animated imports and usage.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::CallExpression, AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Animated"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ImportDeclaration(import) => {
                if import.source.value.as_str() != "react-native" {
                    return;
                }
                let Some(specifiers) = &import.specifiers else { return };
                for spec in specifiers {
                    if let oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(s) = spec
                        && s.imported.name().as_str() == "Animated" {
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, import.span.start as usize);
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line,
                                column,
                                rule_id: super::META.id.into(),
                                message: "Importing `Animated` from 'react-native' — use 'react-native-reanimated' instead.".into(),
                                severity: Severity::Warning,
                                span: None,
                            });
                            return;
                        }
                }
            }
            AstKind::CallExpression(call) => {
                // Matches `Animated.timing(...)`.
                if let Some((prop_name, span_start)) = check_animated_member(&call.callee) {
                    let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`Animated.{prop_name}` is the legacy JS-thread API — use react-native-reanimated."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::NewExpression(new_expr) => {
                // Matches `new Animated.Value(...)`.
                if let Some((prop_name, span_start)) = check_animated_member(&new_expr.callee) {
                    let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`Animated.{prop_name}` is the legacy JS-thread API — use react-native-reanimated."
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

/// Check if an expression is `Animated.timing` or `Animated.Value`.
/// Returns `(property_name, span_start)` if matched.
fn check_animated_member<'a>(expr: &'a oxc_ast::ast::Expression<'a>) -> Option<(&'a str, usize)> {
    let oxc_ast::ast::Expression::StaticMemberExpression(member) = expr else {
        return None;
    };
    let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
        return None;
    };
    if obj.name.as_str() != "Animated" {
        return None;
    }
    let prop_name = member.property.name.as_str();
    if prop_name != "timing" && prop_name != "Value" {
        return None;
    }
    Some((prop_name, member.span.start as usize))
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_animated_timing() {
        let src = "Animated.timing(val, { toValue: 1 }).start();";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_animated_value() {
        let src = "const v = new Animated.Value(0);";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_animated_import() {
        let src = "import { Animated } from 'react-native';";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_reanimated() {
        let src = "import { useSharedValue, withTiming } from 'react-native-reanimated';";
        assert!(run(src).is_empty());
    }
}
