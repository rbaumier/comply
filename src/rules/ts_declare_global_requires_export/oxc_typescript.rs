use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let program = semantic.nodes().program();

        // Find a `declare global` statement.
        let mut declare_global_span = None;
        let mut has_module_marker = false;

        for stmt in &program.body {
            match stmt {
                Statement::TSModuleDeclaration(decl) => {
                    // `declare global { ... }` is parsed as TSModuleDeclaration
                    // with kind Global.
                    let text =
                        &ctx.source[decl.span.start as usize..decl.span.end as usize];
                    if text.starts_with("declare global") && declare_global_span.is_none() {
                        declare_global_span = Some(decl.span);
                    }
                }
                Statement::ExportAllDeclaration(_)
                | Statement::ExportNamedDeclaration(_)
                | Statement::ExportDefaultDeclaration(_) => {
                    has_module_marker = true;
                }
                Statement::ImportDeclaration(_) => {
                    has_module_marker = true;
                }
                _ => {}
            }
        }

        let Some(span) = declare_global_span else {
            return Vec::new();
        };
        if has_module_marker {
            return Vec::new();
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`declare global` only works in module files — add `export {};` to the file."
                .into(),
            severity: Severity::Error,
            span: None,
        }]
    }
}
