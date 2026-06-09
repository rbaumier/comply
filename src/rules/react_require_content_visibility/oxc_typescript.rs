//! OxcCheck backend for react-require-content-visibility.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, JSXElementName};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn in_jsx_expression<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()).skip(1) {
        if matches!(ancestor.kind(), AstKind::JSXExpressionContainer(_)) {
            return true;
        }
    }
    false
}

fn enclosing_virtualizer_tag<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()).skip(1) {
        if let AstKind::JSXOpeningElement(opening) = ancestor.kind() {
            let tag = match &opening.name {
                JSXElementName::Identifier(id) => id.name.as_str(),
                JSXElementName::IdentifierReference(id) => id.name.as_str(),
                _ => continue,
            };
            if tag.contains("Virtual")
                || tag.contains("Virtuoso")
                || tag.contains("Window")
                || tag.ends_with("List")
            {
                return true;
            }
        }
    }
    false
}

fn large_array_source(recv: &Expression, min_nodes: usize) -> bool {
    match recv {
        Expression::ArrayExpression(arr) => {
            let count = arr.elements.iter().count();
            count >= min_nodes
        }
        Expression::CallExpression(call) => {
            // `Array.from({ length: N })`
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            let Expression::Identifier(obj) = &member.object else {
                return false;
            };
            if obj.name.as_str() != "Array" || member.property.name.as_str() != "from" {
                return false;
            }
            let Some(Argument::ObjectExpression(obj_expr)) = call.arguments.first() else {
                return false;
            };
            for prop in &obj_expr.properties {
                if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop
                    && let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &p.key
                        && key.name.as_str() == "length"
                            && let Expression::NumericLiteral(n) = &p.value {
                                return (n.value as usize) >= min_nodes;
                            }
            }
            false
        }
        _ => false,
    }
}

fn callback_body_has_content_visibility(source: &str, span: oxc_span::Span) -> bool {
    let text = &source[span.start as usize..span.end as usize];
    text.contains("contentVisibility") || text.contains("content-visibility")
}

fn walk_expr_for_jsx(e: &Expression) -> bool {
    match e {
        Expression::JSXElement(_) => true,
        Expression::ParenthesizedExpression(p) => walk_expr_for_jsx(&p.expression),
        Expression::ConditionalExpression(c) => {
            walk_expr_for_jsx(&c.consequent) || walk_expr_for_jsx(&c.alternate)
        }
        _ => false,
    }
}

fn callback_returns_jsx_in_body(body: &oxc_ast::ast::FunctionBody) -> bool {
    for stmt in &body.statements {
        match stmt {
            oxc_ast::ast::Statement::ExpressionStatement(es) => {
                if walk_expr_for_jsx(&es.expression) {
                    return true;
                }
            }
            oxc_ast::ast::Statement::ReturnStatement(ret) => {
                if let Some(arg) = &ret.argument
                    && walk_expr_for_jsx(arg) {
                        return true;
                    }
            }
            _ => {}
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let min_nodes =
            ctx.config
                .threshold("react-require-content-visibility", "min_nodes", ctx.lang);

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Must be `.map(...)` call
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "map" {
            return;
        }
        if !in_jsx_expression(node, semantic) {
            return;
        }

        let recv = &member.object;
        let known_large = large_array_source(recv, min_nodes);

        // The rule used to flag any `.map()` returning JSX in a JSX
        // expression, even when the array size was unknown. That
        // misfires on every standard React list pattern. Only flag
        // when we can syntactically PROVE the source has >= min_nodes
        // items (array literal or `Array.from({ length: N })`); the
        // ambiguous case stays silent so non-virtualized lists don't
        // produce a steady stream of speculative warnings.
        if !known_large {
            return;
        }
        if enclosing_virtualizer_tag(node, semantic) {
            return;
        }

        // Find the callback argument
        let Some(first_arg) = call.arguments.first() else {
            return;
        };

        let (returns_jsx, has_cv, cb_span) = match first_arg {
            Argument::ArrowFunctionExpression(arrow) => (
                callback_returns_jsx_in_body(&arrow.body),
                callback_body_has_content_visibility(ctx.source, arrow.span),
                arrow.span,
            ),
            Argument::FunctionExpression(func) => {
                let body = match &func.body {
                    Some(b) => b,
                    None => return,
                };
                (
                    callback_returns_jsx_in_body(body),
                    callback_body_has_content_visibility(ctx.source, func.span()),
                    func.span(),
                )
            }
            _ => return,
        };

        if !returns_jsx {
            return;
        }

        if has_cv {
            return;
        }
        let _ = cb_span;

        let msg = format!(
            "Large list rendered with `.map()` (>= {min_nodes} items) in JSX without \
             virtualization or `contentVisibility: 'auto'` — paints every off-screen row."
        );

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: msg,
            severity: Severity::Warning,
            span: None,
        });
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn ignores_map_on_variable_array() {
        // Regression for rbaumier/comply#20 — the standard React list
        // pattern. Without knowing the size we don't speculate.
        let src = r#"function L({ items }) { return <div>{items.map(item => <Item key={item.id} />)}</div>; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_map_on_small_literal_array() {
        let src = r#"function Tabs() { return <div>{[1,2,3].map(n => <Tab key={n} />)}</div>; }"#;
        assert!(run(src).is_empty());
    }

}
