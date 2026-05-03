//! no-thenable OXC backend — flag objects/classes that define a `then` property.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::ObjectProperty,
            AstType::MethodDefinition,
            AstType::PropertyDefinition,
            AstType::ExportNamedDeclaration,
        ]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["then"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            // Object literal property: `{ then: ... }`
            AstKind::ObjectProperty(prop) => {
                // Only match inside object expressions (not destructuring).
                let parent = semantic.nodes().parent_node(node.id());
                if !matches!(parent.kind(), AstKind::ObjectExpression(_)) {
                    return;
                }
                if is_then_key(&prop.key) {
                    let span = prop.key.span();
                    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "no-thenable".into(),
                        message: "Do not add `then` to an object.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            // Class/object method: method_definition
            AstKind::MethodDefinition(method) => {
                if !is_then_key(&method.key) {
                    return;
                }

                // Check if in class body or object expression.
                let parent = semantic.nodes().parent_node(node.id());
                match parent.kind() {
                    AstKind::ClassBody(_) => {
                        let span = method.key.span();
                        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-thenable".into(),
                            message: "Do not add `then` to a class.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    AstKind::ObjectExpression(_) => {
                        let span = method.key.span();
                        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-thenable".into(),
                            message: "Do not add `then` to an object.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    _ => {}
                }
            }
            // Class field: `class Foo { then = ... }`
            AstKind::PropertyDefinition(prop) => {
                let parent = semantic.nodes().parent_node(node.id());
                if !matches!(parent.kind(), AstKind::ClassBody(_)) {
                    return;
                }
                if is_then_key(&prop.key) {
                    let span = prop.key.span();
                    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "no-thenable".into(),
                        message: "Do not add `then` to a class.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            // Export statements: `export function then() {}` / `export class then {}`
            // and export specifiers: `export { foo as then }`
            AstKind::ExportNamedDeclaration(export) => {
                // Check declaration
                if let Some(ref decl) = export.declaration {
                    match decl {
                        Declaration::FunctionDeclaration(f) => {
                            if f.id.as_ref().is_some_and(|id| id.name.as_str() == "then") {
                                let span = f.id.as_ref().unwrap().span;
                                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: "no-thenable".into(),
                                    message: "Do not export `then`.".into(),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                            }
                        }
                        Declaration::ClassDeclaration(c) => {
                            if c.id.as_ref().is_some_and(|id| id.name.as_str() == "then") {
                                let span = c.id.as_ref().unwrap().span;
                                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: "no-thenable".into(),
                                    message: "Do not export `then`.".into(),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                            }
                        }
                        _ => {}
                    }
                }

                // Check export specifiers: `export { foo as then }`
                for specifier in &export.specifiers {
                    let exported_name = specifier.exported.name().as_str();
                    if exported_name == "then" {
                        let span = specifier.exported.span();
                        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-thenable".into(),
                            message: "Do not export `then`.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

fn is_then_key(key: &PropertyKey) -> bool {
    match key {
        PropertyKey::StaticIdentifier(id) => id.name.as_str() == "then",
        PropertyKey::StringLiteral(s) => s.value.as_str() == "then",
        _ => false,
    }
}
