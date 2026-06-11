//! imports-first OxcCheck backend.
//!
//! Walks all top-level statements via `run_on_semantic`. Import declarations
//! after a non-import statement are flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

/// A directive prologue is an expression statement whose expression is a
/// string literal (e.g. `"use strict";`, `"use client";`).
fn is_directive(stmt: &Statement) -> bool {
    matches!(stmt, Statement::ExpressionStatement(expr)
        if matches!(&expr.expression, Expression::StringLiteral(_)))
}

/// Test-framework configuration calls that are conventionally placed before
/// imports:
/// - `jest.setTimeout(N)` — sets the default test timeout for the file
/// - `vi.setConfig({ testTimeout: N })` — Vitest equivalent
/// - `jasmine.DEFAULT_TIMEOUT_INTERVAL = N` — Jasmine equivalent (assignment)
///
/// These are zero-import-side-effect statements and must not flip `saw_non_import`.
fn is_test_framework_config(stmt: &Statement) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return false;
    };

    // `jest.setTimeout(N)` / `vi.setConfig(...)`
    if let Expression::CallExpression(call) = &expr_stmt.expression
        && let Expression::StaticMemberExpression(member) = &call.callee
        && let Expression::Identifier(obj) = &member.object
    {
        return matches!(
            (obj.name.as_str(), member.property.name.as_str()),
            ("jest", "setTimeout") | ("vi", "setConfig")
        );
    }

    // `jasmine.DEFAULT_TIMEOUT_INTERVAL = N`
    if let Expression::AssignmentExpression(assign) = &expr_stmt.expression
        && let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assign.left
        && let Expression::Identifier(obj) = &member.object
    {
        return obj.name.as_str() == "jasmine"
            && member.property.name.as_str() == "DEFAULT_TIMEOUT_INTERVAL";
    }

    false
}

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
        let body = &semantic.nodes().program().body;
        let mut saw_non_import = false;

        for stmt in body {
            match stmt {
                Statement::ImportDeclaration(_) => {
                    if saw_non_import {
                        let span = match stmt {
                            Statement::ImportDeclaration(d) => d.span,
                            _ => unreachable!(),
                        };
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Import statement after non-import code \u{2014} move to the top of the file.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                // `export { x } from "./y"` re-exports — conventionally live in the
                // import block. Don't flip the flag on them.
                Statement::ExportNamedDeclaration(decl) if decl.source.is_some() => {}
                Statement::ExportAllDeclaration(_) => {}
                // Directives like "use strict" don't count as real code.
                _ if is_directive(stmt) => {}
                // Empty statements (lone semicolons) are harmless.
                Statement::EmptyStatement(_) => {}
                // Test-framework configuration calls (`jest.setTimeout`,
                // `vi.setConfig`, `jasmine.DEFAULT_TIMEOUT_INTERVAL = N`) placed
                // before imports are a widespread convention with no import side
                // effects — they must not flip `saw_non_import`.
                _ if is_test_framework_config(stmt) => {}
                _ => {
                    saw_non_import = true;
                }
            }
        }

        diagnostics
    }
}
