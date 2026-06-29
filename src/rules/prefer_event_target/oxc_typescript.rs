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
                        // `new EventEmitter()` typed as a Node.js stream
                        // (`as Readable`, `: NodeJS.WriteStream`, …) is the
                        // canonical lightweight stream mock: Node streams extend
                        // `EventEmitter`, and `EventTarget` lacks the stream
                        // methods (`write`/`setRawMode`/…), so it cannot satisfy
                        // that contract. The stream type annotation is the
                        // author's signal that `EventEmitter` is mandatory here.
                        if typed_as_node_stream(node.id(), nodes) {
                            continue;
                        }
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
                        // A class implementing a Node.js builtin interface (e.g.
                        // `nodeStream.Readable`) is required to extend
                        // `EventEmitter` to satisfy that interface's contract;
                        // `EventTarget` lacks the emitter API the interface
                        // mandates, so the inheritance is not interchangeable.
                        if implements_node_builtin_interface(class, program) {
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

/// Node.js stream types whose contract extends `EventEmitter`. The match is on
/// the rightmost name segment, so both bare (`Readable`) and namespaced
/// (`NodeJS.WriteStream`) forms are covered. The WHATWG `ReadableStream` /
/// `WritableStream` are deliberately excluded: those are Web Streams that do not
/// extend `EventEmitter`.
const NODE_STREAM_TYPES: &[&str] = &[
    "Readable",
    "Writable",
    "Duplex",
    "Transform",
    "PassThrough",
    "Stream",
    "ReadStream",
    "WriteStream",
];

/// `true` when the node at `node_id` is given a Node.js stream type via a cast
/// (`as`/`satisfies`/`<T>`) or a variable/parameter annotation. Walks the
/// enclosing cast chain (`as unknown as NodeJS.WriteStream`), transparently
/// crossing parentheses, plus the immediate declarator/parameter; stops at the
/// first ancestor that is neither a paren, a cast, nor a typed binding.
fn typed_as_node_stream(node_id: oxc_semantic::NodeId, nodes: &oxc_semantic::AstNodes) -> bool {
    for kind in nodes.ancestor_kinds(node_id) {
        let annotation = match kind {
            // Parentheses around the expression are transparent: keep walking.
            AstKind::ParenthesizedExpression(_) => continue,
            AstKind::TSAsExpression(e) => Some(&e.type_annotation),
            AstKind::TSSatisfiesExpression(e) => Some(&e.type_annotation),
            AstKind::TSTypeAssertion(e) => Some(&e.type_annotation),
            AstKind::VariableDeclarator(decl) => {
                return decl
                    .type_annotation
                    .as_ref()
                    .is_some_and(|a| is_node_stream_type(&a.type_annotation));
            }
            AstKind::FormalParameter(param) => {
                return param
                    .type_annotation
                    .as_ref()
                    .is_some_and(|a| is_node_stream_type(&a.type_annotation));
            }
            _ => return false,
        };
        if let Some(ty) = annotation
            && is_node_stream_type(ty)
        {
            return true;
        }
    }
    false
}

/// `true` when `ty` names a Node.js stream type (matching the rightmost segment
/// of a possibly-qualified name like `NodeJS.WriteStream`).
fn is_node_stream_type(ty: &TSType) -> bool {
    let TSType::TSTypeReference(type_ref) = ty else {
        return false;
    };
    let name = match &type_ref.type_name {
        TSTypeName::IdentifierReference(id) => id.name.as_str(),
        TSTypeName::QualifiedName(qualified) => qualified.right.name.as_str(),
        _ => return false,
    };
    NODE_STREAM_TYPES.contains(&name)
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

/// `true` when the class implements an interface whose name resolves to a
/// binding imported from a `node:*` builtin module. Node's stream/net/http/…
/// interfaces extend `EventEmitter`, so a class implementing one must extend
/// `EventEmitter` to satisfy the contract — `EventTarget` cannot.
fn implements_node_builtin_interface(class: &Class, program: &Program) -> bool {
    class.implements.iter().any(|imp| {
        let local_name = match &imp.expression {
            // `nodeStream.Readable` → the namespace local is `nodeStream`.
            TSTypeName::QualifiedName(qualified) => match &qualified.left {
                TSTypeName::IdentifierReference(id) => id.name.as_str(),
                _ => return false,
            },
            // Bare `Readable` → the name itself (a named import from `node:*`).
            TSTypeName::IdentifierReference(id) => id.name.as_str(),
            _ => return false,
        };
        imports_local_from_node_builtin(program, local_name)
    })
}

/// `true` when `program` has an `import … from "node:*"` declaration binding the
/// local name `local_name` (default, namespace, or named specifier).
fn imports_local_from_node_builtin(program: &Program, local_name: &str) -> bool {
    for stmt in &program.body {
        let Statement::ImportDeclaration(import) = stmt else {
            continue;
        };
        if !import.source.value.as_str().starts_with("node:") {
            continue;
        }
        let Some(ref specifiers) = import.specifiers else {
            continue;
        };
        for s in specifiers {
            let local = match s {
                ImportDeclarationSpecifier::ImportSpecifier(named) => named.local.name.as_str(),
                ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => def.local.name.as_str(),
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => ns.local.name.as_str(),
            };
            if local == local_name {
                return true;
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

    #[test]
    fn allows_new_event_emitter_cast_to_node_stream() {
        // From the issue: vadimdemedes/ink — `new EventEmitter()` cast to a
        // Node.js stream type is the canonical lightweight stream mock.
        assert!(run("const stdin = new EventEmitter() as NodeJS.WriteStream;").is_empty());
    }

    #[test]
    fn allows_new_event_emitter_double_cast_to_node_stream() {
        assert!(
            run("const stdout = new EventEmitter() as unknown as NodeJS.WriteStream;").is_empty()
        );
    }

    #[test]
    fn allows_new_event_emitter_cast_to_bare_stream_type() {
        assert!(run("const r = new EventEmitter() as Readable;").is_empty());
    }

    #[test]
    fn allows_new_event_emitter_with_stream_annotation() {
        assert!(run("const w: Writable = new EventEmitter();").is_empty());
    }

    #[test]
    fn still_flags_plain_new_event_emitter_event_bus() {
        // A generic event bus with no stream contract: EventTarget is viable.
        assert_eq!(run("const bus = new EventEmitter();").len(), 1);
    }

    #[test]
    fn still_flags_new_event_emitter_cast_to_non_stream_type() {
        assert_eq!(run("const bus = new EventEmitter() as MyEmitter;").len(), 1);
    }

    #[test]
    fn allows_new_event_emitter_parenthesized_cast_to_node_stream() {
        // Author-written parentheses must not interrupt the cast-chain walk.
        assert!(run("const s = (new EventEmitter() as unknown) as NodeJS.WriteStream;").is_empty());
    }

    #[test]
    fn allows_new_event_emitter_angle_bracket_assertion_to_node_stream() {
        assert!(run("const r = <Readable>new EventEmitter();").is_empty());
    }

    #[test]
    fn still_flags_new_event_emitter_cast_to_web_readable_stream() {
        // WHATWG `ReadableStream` is a Web Stream, not an EventEmitter.
        assert_eq!(run("const s = new EventEmitter() as ReadableStream;").len(), 1);
    }

    #[test]
    fn _issue_6700() {
        // unjs/unenv: a stub polyfill implementing a Node.js builtin interface
        // (`nodeStream.Readable`, default type import) must extend `EventEmitter`
        // to satisfy the contract; `EventTarget` cannot.
        assert!(
            run(
                "import type nodeStream from \"node:stream\";\nclass _Readable extends EventEmitter implements nodeStream.Readable {}"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_class_implementing_node_builtin_default_import() {
        assert!(
            run(
                "import nodeNet from \"node:net\";\nclass Server extends EventEmitter implements nodeNet.Server {}"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_class_implementing_node_builtin_namespace_import() {
        assert!(
            run(
                "import * as nodeHttp from \"node:http\";\nclass Agent extends EventEmitter implements nodeHttp.Agent {}"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_class_implementing_node_builtin_named_import() {
        assert!(
            run(
                "import { Readable } from \"node:stream\";\nclass _R extends EventEmitter implements Readable {}"
            )
            .is_empty()
        );
    }

    #[test]
    fn still_flags_class_implementing_non_node_import() {
        assert_eq!(
            run(
                "import foo from \"./local\";\nclass Bus extends EventEmitter implements foo.Bar {}"
            )
            .len(),
            1
        );
    }

    #[test]
    fn still_flags_class_implementing_local_interface() {
        assert_eq!(
            run("interface Local {}\nclass Bus extends EventEmitter implements Local {}").len(),
            1
        );
    }
}
