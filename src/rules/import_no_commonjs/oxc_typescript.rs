//! import-no-commonjs oxc backend — forbid CommonJS require/module.exports.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["require", "module.exports"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !crate::rules::module_system::is_es_module_context(ctx.path, ctx.project) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                // Flag `require(...)` calls.
                AstKind::CallExpression(call) => {
                    let Expression::Identifier(id) = &call.callee else { continue };
                    if id.name.as_str() != "require" {
                        continue;
                    }
                    let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Expected `import` instead of `require()`.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                // Flag `module.exports` via assignment expressions.
                AstKind::ExpressionStatement(stmt) => {
                    check_module_exports_in_expr(&stmt.expression, ctx, &mut diagnostics);
                }
                AstKind::VariableDeclarator(decl) => {
                    if let Some(init) = &decl.init {
                        check_module_exports_in_expr(init, ctx, &mut diagnostics);
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

fn check_module_exports_in_expr(
    expr: &Expression,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // `module.exports = ...`
    if let Expression::AssignmentExpression(assign) = expr
        && let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assign.left
            && let Expression::Identifier(obj) = &member.object
                && obj.name.as_str() == "module" && member.property.name.as_str() == "exports" {
                    let (line, column) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Expected `export` or `export default` instead of `module.exports`.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, "module.mjs")
    }


    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, path)
    }


    #[test]
    fn allows_require_when_package_type_is_absent() {
        let d = run_on_path("const x = require('fs');", "server.js");
        assert!(d.is_empty());
    }


    #[test]
    fn flags_require() {
        let d = run_on("const x = require('fs');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("require()"));
    }


    #[test]
    fn flags_module_exports() {
        let d = run_on("module.exports = foo;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("module.exports"));
    }


    #[test]
    fn allows_import() {
        assert!(run_on("import fs from 'fs';").is_empty());
    }
}
