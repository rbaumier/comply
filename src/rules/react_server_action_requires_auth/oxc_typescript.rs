//! react-server-action-requires-auth oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const AUTH_CALLS: &[&str] = &[
    "getSession(",
    "auth()",
    "verifySession",
    "requireAuth",
    "currentUser(",
];

const MUTATION_CALLS: &[&str] = &[".insert(", ".update(", ".delete("];

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["use server"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let src = ctx.source;

        // Quick exit: no mutations in file.
        if !MUTATION_CALLS.iter().any(|c| src.contains(c)) {
            return Vec::new();
        }
        // Quick exit: file already calls auth.
        if AUTH_CALLS.iter().any(|c| src.contains(c)) {
            return Vec::new();
        }

        // Check for "use server" directive.
        if !has_use_server_directive(semantic) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::ExportNamedDeclaration(export) => {
                    let Some(decl) = &export.declaration else {
                        continue;
                    };
                    let oxc_ast::ast::Declaration::FunctionDeclaration(f) = decl else {
                        continue;
                    };
                    if !f.r#async {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, f.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Server Action with mutations must verify authentication before proceeding.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                _ => {}
            }
        }

        diagnostics
    }
}

fn has_use_server_directive(semantic: &oxc_semantic::Semantic) -> bool {
    let program = semantic.nodes().iter().next();
    let Some(root) = program else { return false };
    let AstKind::Program(prog) = root.kind() else {
        return false;
    };
    for directive in &prog.directives {
        if directive.expression.value.as_str() == "use server" {
            return true;
        }
    }
    false
}
