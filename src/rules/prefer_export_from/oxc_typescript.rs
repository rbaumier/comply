use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let program = semantic.nodes().program();
        let mut diagnostics = Vec::new();

        // Phase 1: collect named imports as `local_name -> module_specifier`.
        let mut imports: HashMap<&str, &str> = HashMap::new();
        for stmt in &program.body {
            let Statement::ImportDeclaration(import) = stmt else {
                continue;
            };
            let Some(ref specifiers) = import.specifiers else {
                continue;
            };
            let specifier = import.source.value.as_str();
            for spec in specifiers {
                let ImportDeclarationSpecifier::ImportSpecifier(named) = spec else {
                    continue;
                };
                let local_name = named.local.name.as_str();
                imports.insert(local_name, specifier);
            }
        }

        if imports.is_empty() {
            return diagnostics;
        }

        // Phase 2: find `export { name }` statements (without `from`).
        for stmt in &program.body {
            let Statement::ExportNamedDeclaration(export) = stmt else {
                continue;
            };
            // Skip re-export-from forms — they already use the preferred shape.
            if export.source.is_some() {
                continue;
            }
            // Only look at bare `export { ... }` (no declaration).
            if export.declaration.is_some() {
                continue;
            }
            for spec in &export.specifiers {
                let local_name = spec.local.name().as_str();
                if let Some(module_specifier) = imports.get(local_name) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, spec.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Use `export {{ {local_name} }} from '{module_specifier}'` instead of \
                             importing then re-exporting `{local_name}`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}
