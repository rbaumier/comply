//! OxcCheck backend for avoid-barrel-files.
//!
//! Uses `run_on_semantic` to scan the entire program for re-exports.
//! A file is a barrel when it has >= threshold re-export statements
//! and no other top-level code.

use crate::diagnostic::{Diagnostic, Severity};
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
        let barrel_threshold = ctx.config.threshold("avoid-barrel-files", "min_reexports", ctx.lang);

        let mut reexport_count = 0usize;

        for stmt in &program.body {
            match stmt {
                Statement::ExportNamedDeclaration(decl) => {
                    if decl.source.is_some() {
                        reexport_count += 1;
                    } else {
                        return Vec::new();
                    }
                }
                Statement::ExportAllDeclaration(_) => {
                    reexport_count += 1;
                }
                Statement::ExportDefaultDeclaration(_) => {
                    return Vec::new();
                }
                _ => {
                    return Vec::new();
                }
            }
        }

        if reexport_count < barrel_threshold {
            return Vec::new();
        }

        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: format!(
                "Barrel file — {reexport_count} re-exports and no other code. Import directly from source modules."
            ),
            severity: Severity::Warning,
            span: None,
        }]
    }
}
