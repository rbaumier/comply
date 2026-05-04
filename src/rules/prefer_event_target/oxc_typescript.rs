use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

/// Packages whose `EventEmitter` is a different class.
const IGNORED_PACKAGES: &[&str] = &["@angular/core", "eventemitter3"];

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["EventEmitter"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let program = semantic.nodes().program();
        let mut diagnostics = Vec::new();

        // Check if EventEmitter is imported from an ignored package.
        if imports_event_emitter_from_ignored(program) {
            return diagnostics;
        }

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::NewExpression(new_expr) => {
                    let Expression::Identifier(id) = &new_expr.callee else {
                        continue;
                    };
                    if id.name.as_str() == "EventEmitter" {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Prefer `EventTarget` over `EventEmitter`.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                AstKind::Class(class) => {
                    if let Some(ref super_class) = class.super_class
                        && let Expression::Identifier(id) = super_class
                            && id.name.as_str() == "EventEmitter" {
                                let (line, column) =
                                    byte_offset_to_line_col(ctx.source, id.span.start as usize);
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: super::META.id.into(),
                                    message: "Prefer `EventTarget` over `EventEmitter`.".into(),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                            }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

fn imports_event_emitter_from_ignored(program: &oxc_ast::ast::Program) -> bool {
    for stmt in &program.body {
        let Statement::ImportDeclaration(import) = stmt else {
            continue;
        };
        let spec = import.source.value.as_str();
        if !IGNORED_PACKAGES.contains(&spec) {
            continue;
        }
        let Some(ref specifiers) = import.specifiers else {
            continue;
        };
        for s in specifiers {
            match s {
                ImportDeclarationSpecifier::ImportSpecifier(named) => {
                    if named.local.name.as_str() == "EventEmitter" {
                        return true;
                    }
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => {
                    if def.local.name.as_str() == "EventEmitter" {
                        return true;
                    }
                }
                _ => {}
            }
        }
    }
    false
}
