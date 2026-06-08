//! OxcCheck backend for react-display-name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::ExportDefaultDeclaration(export) = node.kind() else { continue };

            let anonymous_span = match &export.declaration {
                ExportDefaultDeclarationKind::ArrowFunctionExpression(arrow) => {
                    if contains_jsx(arrow.span, semantic) {
                        Some(arrow.span)
                    } else {
                        None
                    }
                }
                ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                    if func.id.is_some() {
                        None
                    } else if contains_jsx(func.span, semantic) {
                        Some(func.span)
                    } else {
                        None
                    }
                }
                ExportDefaultDeclarationKind::CallExpression(call) => {
                    if is_react_wrapper_call(call) {
                        call.arguments.first().and_then(|arg| {
                            let expr = arg.as_expression()?;
                            match expr {
                                Expression::ArrowFunctionExpression(arrow) => {
                                    if contains_jsx(arrow.span, semantic) {
                                        Some(arrow.span)
                                    } else {
                                        None
                                    }
                                }
                                Expression::FunctionExpression(func) => {
                                    if func.id.is_some() {
                                        None
                                    } else if contains_jsx(func.span, semantic) {
                                        Some(func.span)
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            }
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            };

            let Some(span) = anonymous_span else { continue };
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Anonymous React component missing a display name.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

fn is_react_wrapper_call(call: &CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => id.name == "memo" || id.name == "forwardRef",
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name == "React"
                    && (member.property.name == "memo" || member.property.name == "forwardRef")
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Check if any JSX node exists within the given span by scanning semantic nodes.
fn contains_jsx(outer: oxc_span::Span, semantic: &oxc_semantic::Semantic) -> bool {
    for node in semantic.nodes().iter() {
        if let AstKind::JSXOpeningElement(el) = node.kind() {
            if el.span.start >= outer.start && el.span.end <= outer.end {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_anonymous_arrow_default_export() {
        let d = run_on("export default () => <div />;");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_anonymous_memo_default_export() {
        let d = run_on("export default React.memo(() => <div />);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_anonymous_forward_ref_default_export() {
        let d = run_on("export default React.forwardRef((props, ref) => <div ref={ref} />);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_bare_memo_default_export() {
        let d = run_on("export default memo(() => <div />);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_named_function_default_export() {
        assert!(run_on("export default function MyComponent() { return <div />; }").is_empty());
    }


    #[test]
    fn allows_named_arrow_then_export() {
        let src = "const Foo = () => <div />;\nexport default Foo;";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_memo_with_named_component() {
        let src = "const Foo = () => <div />;\nexport default React.memo(Foo);";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_jsx_default_export() {
        assert!(run_on("export default () => 42;").is_empty());
    }
}
