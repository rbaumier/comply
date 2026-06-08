use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Function,
            AstType::ArrowFunctionExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::Function(func) => {
                // Explicit return type annotation — TS enforces it. Don't
                // second-guess with text-only inference.
                if func.return_type.is_some() {
                    return;
                }
                let Some(body) = &func.body else { return };
                let mut return_types = FxHashSet::default();
                collect_return_types_from_stmts(&body.statements, &mut return_types);
                if let Some(diag) = check_return_types(&return_types, ctx, func.span.start as usize) {
                    diagnostics.push(diag);
                }
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                // Skip arrow functions with expression body (single return type).
                if arrow.expression {
                    return;
                }
                // Same as above: trust an explicit return-type annotation.
                if arrow.return_type.is_some() {
                    return;
                }
                // Pattern: `const fn: Type = async (args) => { … }`
                // The annotation lives on the VariableDeclarator, not the arrow.
                // Treat it as explicit — TS already enforces consistency.
                if let AstKind::VariableDeclarator(decl) =
                    semantic.nodes().parent_node(node.id()).kind()
                {
                    if decl.type_annotation.is_some() {
                        return;
                    }
                }
                let mut return_types = FxHashSet::default();
                collect_return_types_from_stmts(&arrow.body.statements, &mut return_types);
                if let Some(diag) = check_return_types(&return_types, ctx, arrow.span.start as usize) {
                    diagnostics.push(diag);
                }
            }
            _ => {}
        }
    }
}

fn check_return_types(
    return_types: &FxHashSet<&str>,
    ctx: &CheckCtx,
    span_start: usize,
) -> Option<Diagnostic> {
    if return_types.len() < 2 {
        return None;
    }
    // If any return is "unknown" — i.e. we have no syntactic evidence
    // of its concrete type — we cannot prove inconsistency, so the
    // call has to defer to the TS type checker. Without this, common
    // shapes like `useMemo<T[]>(() => data?.items ?? [])` and
    // `typeof input` (always a string literal type) produce a steady
    // stream of "Function returns inconsistent types: {unknown, array}"
    // false positives.
    if return_types.contains("unknown") {
        return None;
    }
    let has_null_or_undefined =
        return_types.contains("null") || return_types.contains("undefined");
    let non_nullish: Vec<_> = return_types
        .iter()
        .filter(|&&t| t != "null" && t != "undefined")
        .collect();
    if has_null_or_undefined && non_nullish.len() <= 1 {
        return None;
    }
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
    Some(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!("Function returns inconsistent types: {:?}", return_types),
        severity: Severity::Warning,
        span: None,
    })
}

fn collect_return_types_from_stmts<'a>(
    stmts: &'a [Statement<'a>],
    types: &mut FxHashSet<&'static str>,
) {
    for stmt in stmts {
        collect_return_types_from_stmt(stmt, types);
    }
}

fn collect_return_types_from_stmt<'a>(
    stmt: &'a Statement<'a>,
    types: &mut FxHashSet<&'static str>,
) {
    match stmt {
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                types.insert(infer_type(arg));
            }
        }
        // Don't descend into nested functions.
        Statement::FunctionDeclaration(_) => {}
        Statement::ExpressionStatement(expr) => {
            // Check for arrow/function expressions but don't descend.
            match &expr.expression {
                Expression::ArrowFunctionExpression(_)
                | Expression::FunctionExpression(_) => {}
                _ => collect_return_types_from_expr(&expr.expression, types),
            }
        }
        Statement::BlockStatement(block) => {
            collect_return_types_from_stmts(&block.body, types);
        }
        Statement::IfStatement(if_stmt) => {
            collect_return_types_from_stmt(&if_stmt.consequent, types);
            if let Some(alt) = &if_stmt.alternate {
                collect_return_types_from_stmt(alt, types);
            }
        }
        Statement::SwitchStatement(switch) => {
            for case in &switch.cases {
                collect_return_types_from_stmts(&case.consequent, types);
            }
        }
        Statement::TryStatement(try_stmt) => {
            collect_return_types_from_stmts(&try_stmt.block.body, types);
            if let Some(handler) = &try_stmt.handler {
                collect_return_types_from_stmts(&handler.body.body, types);
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                collect_return_types_from_stmts(&finalizer.body, types);
            }
        }
        Statement::ForStatement(f) => {
            collect_return_types_from_stmt(&f.body, types);
        }
        Statement::WhileStatement(w) => {
            collect_return_types_from_stmt(&w.body, types);
        }
        Statement::ForInStatement(f) => {
            collect_return_types_from_stmt(&f.body, types);
        }
        Statement::ForOfStatement(f) => {
            collect_return_types_from_stmt(&f.body, types);
        }
        Statement::DoWhileStatement(d) => {
            collect_return_types_from_stmt(&d.body, types);
        }
        Statement::LabeledStatement(l) => {
            collect_return_types_from_stmt(&l.body, types);
        }
        _ => {}
    }
}

fn collect_return_types_from_expr<'a>(
    _expr: &'a Expression<'a>,
    _types: &mut FxHashSet<&'static str>,
) {
    // Expression statements don't contain return statements at the top level.
}

fn infer_type(expr: &Expression) -> &'static str {
    match expr {
        Expression::NumericLiteral(_) => "number",
        Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => "string",
        Expression::BooleanLiteral(_) => "boolean",
        Expression::NullLiteral(_) => "null",
        Expression::ArrayExpression(_) => "array",
        Expression::ObjectExpression(_) => "object",
        Expression::Identifier(id) if id.name == "undefined" => "undefined",
        // `typeof x` always evaluates to a string literal type — treat
        // it as a string so functions mixing literal-string returns and
        // a final `return typeof input;` don't trip the rule.
        Expression::UnaryExpression(unary)
            if unary.operator == oxc_ast::ast::UnaryOperator::Typeof =>
        {
            "string"
        }
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_obvious_mixed_returns() {
        let src = r#"
            function f(x: boolean) {
                if (x) return 1;
                return "two";
            }
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn ignores_typeof_with_string_literals() {
        // Regression for rbaumier/comply#18 — typeof always yields a
        // string literal type.
        let src = r#"
            function inputTypeToken(input: unknown): string {
                if (input === null) return "null";
                if (Array.isArray(input)) return "array";
                if (input instanceof Date) return "date";
                if (typeof input === "number" && Number.isNaN(input)) return "nan";
                return typeof input;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_function_with_explicit_return_type_annotation() {
        // Regression for #70 — explicit `Promise<Option[]>` annotation
        // means TS already enforces consistency; the rule shouldn't fire.
        let src = r#"
            const fn = async (query: string): Promise<Option[]> => {
                try {
                    const results = await search(query);
                    return results.map((entity) => toOption(entity));
                } catch {
                    return [];
                }
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_arrow_with_type_annotation_on_declarator() {
        // Regression for #361 — annotation on the `const fn: Type =` declarator
        // rather than on the arrow should be trusted as explicit.
        let src = r#"
            const fn: (query: string) => Promise<Option[]> = async (query) => {
                try {
                    return await doSomething(query);
                } catch {
                    return [];
                }
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_unknown_when_present() {
        // Regression for rbaumier/comply#25 — useMemo<T[]>(() => data?.items ?? []).
        let src = r#"
            const items = useMemo(() => {
                if (level === "org") return query.data?.items ?? [];
                if (level === "team") return query2.data?.items ?? [];
                return [];
            }, []);
        "#;
        assert!(run(src).is_empty());
    }
}
