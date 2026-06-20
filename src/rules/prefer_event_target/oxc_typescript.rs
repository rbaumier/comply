use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::Span;
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

        let nodes = semantic.nodes();
        for node in nodes.iter() {
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
                        && id.name.as_str() == "EventEmitter"
                    {
                        // A class that calls `this.emit(...)` is an active event
                        // source exposing `EventEmitter`'s emitter contract
                        // (`.on()`/`.emit()`) as its public API. `EventTarget`
                        // dispatches with `dispatchEvent(new Event(...))` and has
                        // no `.emit()`, so swapping it would break consumers.
                        if class_emits(class.span, nodes) {
                            continue;
                        }
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

/// `true` when a `this.emit(...)` call appears inside the class spanned by
/// `class_span`. Such a call proves the class is an active event source built on
/// `EventEmitter`'s emitter contract, which `EventTarget` cannot provide.
fn class_emits(class_span: Span, nodes: &oxc_semantic::AstNodes) -> bool {
    nodes.iter().any(|node| {
        let AstKind::CallExpression(call) = node.kind() else {
            return false;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return false;
        };
        member.property.name.as_str() == "emit"
            && matches!(member.object, Expression::ThisExpression(_))
            && class_span.contains_inclusive(call.span)
    })
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

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_plain_new_event_emitter() {
        assert_eq!(run("const bus = new EventEmitter();").len(), 1);
    }

    #[test]
    fn flags_class_extending_event_emitter() {
        assert_eq!(run("class Bus extends EventEmitter {}").len(), 1);
    }

    #[test]
    fn allows_exported_class_emitting_named_events() {
        // From the issue: tj/commander.js — `Command extends EventEmitter`
        // exposes `.emit()`/`.on()` as its public API.
        assert!(
            run(
                "export class Command extends EventEmitter {\n  parse() {\n    this.emit('command:run');\n  }\n}"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_class_emitting_in_nested_block() {
        assert!(
            run(
                "class Bus extends EventEmitter {\n  fire(ok) {\n    if (ok) {\n      this.emit('done');\n    }\n  }\n}"
            )
            .is_empty()
        );
    }

    #[test]
    fn still_flags_class_extending_event_emitter_without_emit() {
        // No `this.emit(...)`: a plain alias of EventEmitter that EventTarget can replace.
        assert_eq!(
            run("class Bus extends EventEmitter {\n  add(fn) {\n    this.on('x', fn);\n  }\n}").len(),
            1
        );
    }
}
