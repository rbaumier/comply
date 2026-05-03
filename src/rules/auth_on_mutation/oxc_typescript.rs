//! auth-on-mutation OxcCheck backend — mutation route handlers (POST/PUT/DELETE/PATCH)
//! should reference auth.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;

pub struct Check;

const MUTATION_METHODS: &[&str] = &["post", "put", "delete", "patch"];
const AUTH_KEYWORDS: &[&str] = &[
    "auth",
    "token",
    "session",
    "middleware",
    "guard",
    "protect",
    "verify",
];

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    if TEST_MARKERS.iter().any(|m| s.contains(m)) {
        return true;
    }
    path.components()
        .any(|c| c.as_os_str() == "tests" || c.as_os_str() == "e2e")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if is_test_file(ctx.path) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Match `app.post(`, `app.put(`, etc.
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !MUTATION_METHODS.contains(&method) {
            return;
        }

        // Check the full call expression text for auth keywords.
        let call_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let lower = call_text.to_lowercase();
        if AUTH_KEYWORDS.iter().any(|k| lower.contains(k)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "auth-on-mutation".into(),
            message: "Mutation route without auth check — add authentication/authorization.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
