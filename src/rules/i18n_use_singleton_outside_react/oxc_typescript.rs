//! i18n-use-singleton-outside-react oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

/// A React component is a function whose name starts with an uppercase letter
/// (PascalCase).
fn is_react_component_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useTranslation"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "useTranslation" {
            return;
        }

        // Walk ancestors to find the enclosing function/component.
        let mut in_react_component = false;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            match ancestor.kind() {
                AstKind::Function(func) => {
                    if let Some(id) = &func.id {
                        in_react_component = is_react_component_name(id.name.as_str());
                    }
                    break;
                }
                AstKind::VariableDeclarator(decl) => {
                    // Check if the value is an arrow function or function expression.
                    let is_func = decl.init.as_ref().is_some_and(|init| {
                        matches!(
                            init,
                            Expression::ArrowFunctionExpression(_)
                                | Expression::FunctionExpression(_)
                        )
                    });
                    if is_func {
                        if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &decl.id {
                            in_react_component = is_react_component_name(id.name.as_str());
                        }
                        break;
                    }
                }
                AstKind::MethodDefinition(_) => {
                    break;
                }
                _ => {}
            }
        }

        if in_react_component {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "useTranslation() must only run inside a React component. Use the `i18n.t()` singleton here.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_outside_component() {
        let src = "function head() { const { t } = useTranslation(); return t('x'); }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_inside_component() {
        let src = "function MyComponent() { const { t } = useTranslation(); return null; }";
        assert!(run(src).is_empty());
    }
}
