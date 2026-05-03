use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, ObjectPropertyKind};
use std::sync::Arc;

const MUTATION_PREFIXES: &[&str] = &["create", "update", "delete", "login", "logout"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["createServerFn"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclaration(decl) = node.kind() else { return };

        for declarator in &decl.declarations {
            let BindingPattern::BindingIdentifier(ref id) = declarator.id else { continue };
            let name = id.name.as_str();
            let lower = name.to_ascii_lowercase();
            if !MUTATION_PREFIXES.iter().any(|p| lower.starts_with(p)) {
                continue;
            }

            let Some(ref init) = declarator.init else { continue };
            let Some((call_span, args)) = find_create_server_fn_call(init) else { continue };

            let has_post = args.iter().any(|arg| {
                let Some(expr) = arg.as_expression() else { return false };
                let Expression::ObjectExpression(obj) = expr else { return false };
                obj.properties.iter().any(|prop| {
                    let ObjectPropertyKind::ObjectProperty(p) = prop else { return false };
                    let key_name = p.key.name();
                    let Some(key) = key_name else { return false };
                    if key != "method" { return false; }
                    let Expression::StringLiteral(val) = &p.value else { return false };
                    val.value.as_str().eq_ignore_ascii_case("POST")
                })
            });

            if has_post {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, call_span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Server function `{name}` is named like a mutation but does not \
                     declare `method: 'POST'`. Mutations should not be GET-accessible."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Find a `createServerFn(...)` call in the init expression, handling chained
/// calls like `createServerFn({...}).handler(fn)`.
fn find_create_server_fn_call<'a>(
    expr: &'a Expression<'a>,
) -> Option<(oxc_span::Span, &'a oxc_allocator::Vec<'a, oxc_ast::ast::Argument<'a>>)> {
    match expr {
        Expression::CallExpression(call) => {
            match &call.callee {
                Expression::Identifier(id) if id.name.as_str() == "createServerFn" => {
                    Some((call.span, &call.arguments))
                }
                _ => {
                    // Could be `createServerFn({}).handler(...)` — recurse into callee.
                    find_create_server_fn_call(&call.callee)
                }
            }
        }
        Expression::StaticMemberExpression(member) => {
            find_create_server_fn_call(&member.object)
        }
        _ => None,
    }
}
