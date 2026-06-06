//! react-server-action-requires-validation oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const VALIDATOR_CALLS: &[&str] = &[".parse(", ".safeParse(", ".input("];

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

        // Quick exit: file already calls a validator.
        if VALIDATOR_CALLS.iter().any(|c| ctx.source_contains(c)) {
            return Vec::new();
        }

        // Check for "use server" directive in first few statements.
        if !has_use_server_directive(semantic) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            if let AstKind::ExportNamedDeclaration(export) = node.kind() {
                let Some(decl) = &export.declaration else {
                    continue;
                };
                let oxc_ast::ast::Declaration::FunctionDeclaration(f) = decl else {
                    continue;
                };
                if !f.r#async {
                    continue;
                }
                if !has_params(f) {
                    continue;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, f.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Server Action with parameters must validate input with `.parse()` or `.safeParse()`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
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

fn has_params(f: &oxc_ast::ast::Function) -> bool {
    !f.params.items.is_empty()
}
