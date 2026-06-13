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
                let AstKind::ObjectExpression(obj) = parent.kind() else {
                    return;
                };
                if is_then_key(&prop.key) {
                    if is_intentional_thenable(is_canonical_then_value(&prop.value), || {
                        object_is_promise_like(obj)
                    }) {
                        return;
                    }
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
                let canonical = is_canonical_then_params(&method.value.params);
                match parent.kind() {
                    AstKind::ClassBody(class_body) => {
                        if is_intentional_thenable(canonical, || {
                            class_is_promise_like(class_body)
                        }) {
                            return;
                        }
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
                    AstKind::ObjectExpression(obj) => {
                        if is_intentional_thenable(canonical, || object_is_promise_like(obj)) {
                            return;
                        }
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
                let AstKind::ClassBody(class_body) = parent.kind() else {
                    return;
                };
                if is_then_key(&prop.key) {
                    let canonical = prop
                        .value
                        .as_ref()
                        .is_some_and(is_canonical_then_value);
                    if is_intentional_thenable(canonical, || class_is_promise_like(class_body)) {
                        return;
                    }
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
    key_name(key) == Some("then")
}

/// The static name of a property key, or `None` for computed/private keys.
fn key_name<'a>(key: &'a PropertyKey) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// A `then` is intentional — not an accidental thenable — when it carries the
/// canonical `then(onfulfilled, onrejected)` signature (e.g. an awaitable
/// `StreamableMethod`), or when its host also declares `catch` and `finally`
/// (a full Promise-like interface, e.g. an Azure LRO `PollerLike`).
fn is_intentional_thenable(canonical_signature: bool, host_is_promise_like: impl FnOnce() -> bool) -> bool {
    canonical_signature || host_is_promise_like()
}

/// Whether a property value is a function with the canonical two-parameter
/// `then(onfulfilled, onrejected)` signature.
fn is_canonical_then_value(value: &Expression) -> bool {
    match value {
        Expression::FunctionExpression(func) => is_canonical_then_params(&func.params),
        Expression::ArrowFunctionExpression(arrow) => is_canonical_then_params(&arrow.params),
        _ => false,
    }
}

/// Whether a parameter list is the canonical two handlers `(onfulfilled, onrejected)`.
fn is_canonical_then_params(params: &FormalParameters) -> bool {
    params.rest.is_none() && params.items.len() == 2
}

/// Whether an object literal also declares `catch` and `finally` siblings.
fn object_is_promise_like(obj: &ObjectExpression) -> bool {
    let mut has_catch = false;
    let mut has_finally = false;
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        match key_name(&p.key) {
            Some("catch") => has_catch = true,
            Some("finally") => has_finally = true,
            _ => {}
        }
    }
    has_catch && has_finally
}

/// Whether a class body also declares `catch` and `finally` members.
fn class_is_promise_like(body: &ClassBody) -> bool {
    let mut has_catch = false;
    let mut has_finally = false;
    for element in &body.body {
        let name = match element {
            ClassElement::MethodDefinition(m) => key_name(&m.key),
            ClassElement::PropertyDefinition(p) => key_name(&p.key),
            ClassElement::AccessorProperty(a) => key_name(&a.key),
            _ => None,
        };
        match name {
            Some("catch") => has_catch = true,
            Some("finally") => has_finally = true,
            _ => {}
        }
    }
    has_catch && has_finally
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_object_with_then_method() {
        let d = run_on("const obj = { then() {} };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("object"));
    }

    #[test]
    fn flags_object_with_then_property() {
        let d = run_on("const obj = { then: function() {} };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("object"));
    }

    #[test]
    fn flags_class_with_then_method() {
        let d = run_on("class Foo { then() {} }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("class"));
    }

    #[test]
    fn flags_class_with_then_field() {
        let d = run_on("class Foo { then = 42; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("class"));
    }

    #[test]
    fn flags_exported_function_then() {
        let d = run_on("export function then() {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("export"));
    }

    #[test]
    fn allows_object_without_then() {
        assert!(run_on("const obj = { foo() {} };").is_empty());
    }

    // ── Issue #1153: intentional thenables must not flag ──────────────

    #[test]
    fn allows_poller_like_then_catch_finally_triple() {
        // Azure LRO `PollerLike`: then/catch/finally make the poller awaitable
        // while still allowing manual polling. Full Promise-like interface.
        let src = r#"
const poller = {
  then(onfulfilled, onrejected) {
    return poller.pollUntilDone().then(onfulfilled, onrejected);
  },
  catch(onrejected) {
    return poller.pollUntilDone().catch(onrejected);
  },
  finally(onfinally) {
    return poller.pollUntilDone().finally(onfinally);
  },
};
"#;
        let d = run_on(src);
        assert!(d.is_empty(), "then/catch/finally triple must be allowed: {d:?}");
    }

    #[test]
    fn allows_streamable_method_canonical_lone_then() {
        // Azure `StreamableMethod`: a lone `then(onFulfilled, onrejected)` with
        // the canonical two-handler signature that forwards a Promise.
        let src = r#"
function getClient() {
  return {
    then: function (onFulfilled, onrejected) {
      return sendRequest(method, url, pipeline).then(onFulfilled, onrejected);
    },
  };
}
"#;
        let d = run_on(src);
        assert!(d.is_empty(), "canonical lone `then` must be allowed: {d:?}");
    }

    #[test]
    fn flags_accidental_lone_then_arrow() {
        // Accidental thenable: a lone `then` that does something unrelated and
        // does not carry the canonical (onfulfilled, onrejected) signature.
        let d = run_on("const obj = { then: () => doSomethingUnrelated() };");
        assert_eq!(d.len(), 1, "accidental thenable must still flag: {d:?}");
        assert!(d[0].message.contains("object"));
    }

    #[test]
    fn flags_lone_then_with_single_param() {
        // A single-parameter `then` is not the canonical Promise signature.
        let d = run_on("const obj = { then(cb) { cb(); } };");
        assert_eq!(d.len(), 1, "single-param `then` must still flag: {d:?}");
    }

    #[test]
    fn flags_then_with_catch_but_no_finally() {
        // Missing `finally` — not a full Promise-like interface, and the `then`
        // is non-canonical (zero params), so it is still an accidental thenable.
        let src = r#"
const obj = {
  then() { return run(); },
  catch() { return handle(); },
};
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1, "then+catch without finally must still flag: {d:?}");
    }

    #[test]
    fn allows_canonical_then_class_method() {
        // A class `then(onfulfilled, onrejected)` with the canonical signature.
        let d = run_on("class Awaitable { then(onfulfilled, onrejected) { return p.then(onfulfilled, onrejected); } }");
        assert!(d.is_empty(), "canonical class `then` must be allowed: {d:?}");
    }
}
