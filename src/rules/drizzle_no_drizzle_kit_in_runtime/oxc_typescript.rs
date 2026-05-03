use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_config_or_migration_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("drizzle.config")
        || s.contains("/migrate")
        || s.contains("/migrations/")
        || s.ends_with("migrate.ts")
        || s.ends_with("migrate.js")
        || s.ends_with("migrate.mjs")
}

fn module_is_drizzle_kit(spec: &str) -> bool {
    spec == "drizzle-kit" || spec.starts_with("drizzle-kit/")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["drizzle-kit"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if is_config_or_migration_file(ctx.path) {
            return;
        }
        match node.kind() {
            AstKind::ImportDeclaration(import) => {
                if !module_is_drizzle_kit(import.source.value.as_str()) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, import.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`drizzle-kit` is a dev-time CLI — importing it from runtime code bloats the production bundle.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::CallExpression(call) => {
                let oxc_ast::ast::Expression::Identifier(callee) = &call.callee else {
                    return;
                };
                if callee.name != "require" {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else {
                    return;
                };
                let oxc_ast::ast::Argument::StringLiteral(lit) = first_arg else {
                    return;
                };
                if !module_is_drizzle_kit(lit.value.as_str()) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`require('drizzle-kit')` in runtime code — keep migration tooling out of the production bundle.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}
