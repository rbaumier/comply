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
                    // A `then` whose value is not a function (numeric/string/null
                    // literal, identifier, member expression, etc.) is plain data,
                    // e.g. MongoDB aggregation `$cond`/`$switch` branches. Such an
                    // object cannot be accidentally awaited, so it is not a thenable.
                    if !is_function_value(&prop.value) {
                        return;
                    }
                    if is_intentional_thenable(is_canonical_then_value(&prop.value), || {
                        object_is_promise_like(obj)
                            || object_returned_from_thenable_typed_fn(parent.id(), semantic)
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
                        if is_intentional_thenable(canonical, || {
                            object_is_promise_like(obj)
                                || object_returned_from_thenable_typed_fn(parent.id(), semantic)
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
/// `StreamableMethod`), or when its host is promise-like: it declares `catch`
/// and `finally` (a full Promise-like interface, e.g. an Azure LRO `PollerLike`)
/// or is an object literal returned from a function whose return type is
/// declared `Thenable` (e.g. zustand's `toThenable` sync-to-async bridge).
fn is_intentional_thenable(canonical_signature: bool, host_is_promise_like: impl FnOnce() -> bool) -> bool {
    canonical_signature || host_is_promise_like()
}

/// Whether a property value is a function expression or arrow function. Only a
/// function-valued `then` makes an object an accidental thenable; a `then`
/// holding any other value (literal, identifier, member, object/array, …) is
/// plain data and `await` resolves to the host object itself.
fn is_function_value(value: &Expression) -> bool {
    matches!(
        value,
        Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_)
    )
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

/// Whether the object literal at `object_id` *is the returned value* of the
/// nearest enclosing function/arrow whose return-type annotation references
/// `Thenable`. Such a function explicitly declares it produces a thenable, so a
/// minimal `then`-bearing object it returns is intentional — e.g. zustand's
/// `(input): Thenable<Result> => { ... return { then(onFulfilled) {…} } }`.
///
/// The object qualifies only when it is the direct returned operand: either the
/// `argument` of a `return` statement or the expression body of an arrow. It may
/// be reached through transparent wrappers — `as`/`satisfies` casts, non-null
/// `!`, parentheses, and `ConditionalExpression` consequent/alternate branches.
/// The walk stops (not in return position) at the first non-transparent parent:
/// a call argument (`return wrap({ then })`), a nested object/array property or
/// element (`return { a: { then } }`, `return [{ then }]`), or a conditional
/// `test`. The type reference is matched inside unions (`A | Thenable<B>`) and
/// parentheses, so those return types still qualify.
fn object_returned_from_thenable_typed_fn(
    object_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = object_id;
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        match nodes.kind(parent_id) {
            // Transparent expression wrappers — keep climbing toward the return.
            AstKind::TSAsExpression(_)
            | AstKind::TSSatisfiesExpression(_)
            | AstKind::TSNonNullExpression(_)
            | AstKind::ParenthesizedExpression(_) => {}
            // A ternary is transparent only through its result branches; if the
            // object is the `test`, it is not the returned value.
            AstKind::ConditionalExpression(cond) => {
                if cond.test.span() == nodes.kind(current_id).span() {
                    return false;
                }
            }
            // The object is the `return` argument (modulo transparent wrappers).
            AstKind::ReturnStatement(_) => {
                return enclosing_fn_return_type_is_thenable(parent_id, nodes);
            }
            // Arrow expression body: `(): Thenable<T> => ({ then })`.
            AstKind::ExpressionStatement(_) => {
                return arrow_expression_body_return_type_is_thenable(parent_id, nodes);
            }
            // Any other parent (call argument, object/array property/element,
            // declarator, …) means the object is not the returned value.
            _ => return false,
        }
        current_id = parent_id;
    }
}

/// Whether the function/arrow enclosing the return statement at `return_id`
/// declares a `Thenable` return type.
fn enclosing_fn_return_type_is_thenable(
    return_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    for ancestor in nodes.ancestors(return_id) {
        match ancestor.kind() {
            AstKind::Function(f) => return fn_return_type_is_thenable(f.return_type.as_deref()),
            AstKind::ArrowFunctionExpression(a) => {
                return fn_return_type_is_thenable(a.return_type.as_deref());
            }
            _ => {}
        }
    }
    false
}

/// Whether the expression statement at `stmt_id` is the implicit-return body of
/// an arrow (`() => ({ … })`) whose return type references `Thenable`. The
/// statement must sit directly in a `FunctionBody` that is the body of an arrow
/// with an expression body, otherwise it is an incidental statement, not a
/// return.
fn arrow_expression_body_return_type_is_thenable(
    stmt_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    let body_id = nodes.parent_id(stmt_id);
    if !matches!(nodes.kind(body_id), AstKind::FunctionBody(_)) {
        return false;
    }
    let arrow_id = nodes.parent_id(body_id);
    let AstKind::ArrowFunctionExpression(arrow) = nodes.kind(arrow_id) else {
        return false;
    };
    arrow.expression && fn_return_type_is_thenable(arrow.return_type.as_deref())
}

/// Whether a function/arrow return-type annotation references `Thenable`.
fn fn_return_type_is_thenable(annotation: Option<&TSTypeAnnotation>) -> bool {
    annotation.is_some_and(|ann| type_references_thenable(&ann.type_annotation))
}

/// Whether a type annotation is, or (within a union or parentheses) contains, a
/// reference to the `Thenable` type.
fn type_references_thenable(ty: &TSType) -> bool {
    match ty {
        TSType::TSTypeReference(r) => type_name_is_thenable(&r.type_name),
        TSType::TSUnionType(u) => u.types.iter().any(type_references_thenable),
        TSType::TSParenthesizedType(p) => type_references_thenable(&p.type_annotation),
        _ => false,
    }
}

/// Whether a type name's trailing identifier is `Thenable` (covers both
/// `Thenable` and a qualified `ns.Thenable`).
fn type_name_is_thenable(name: &TSTypeName) -> bool {
    match name {
        TSTypeName::IdentifierReference(id) => id.name.as_str() == "Thenable",
        TSTypeName::QualifiedName(q) => q.right.name.as_str() == "Thenable",
        TSTypeName::ThisExpression(_) => false,
    }
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

    // ── Issue #2329: a `then` whose value is not a function is plain data ──

    #[test]
    fn allows_object_then_numeric_literal_value() {
        // MongoDB aggregation `$cond`: `then` is a numeric literal, not a
        // function — the object can never be accidentally awaited.
        let d = run_on("const e = { $cond: { if: '$x', then: 0.9, else: 1 } };");
        assert!(d.is_empty(), "numeric `then` value must be allowed: {d:?}");
    }

    #[test]
    fn allows_object_then_string_literal_value() {
        // MongoDB aggregation `$switch` branch: `then` is a string literal.
        let d = run_on("const b = { case: cond, then: 'Detlef' };");
        assert!(d.is_empty(), "string `then` value must be allowed: {d:?}");
    }

    #[test]
    fn allows_object_then_null_value() {
        let d = run_on("const obj = { then: null };");
        assert!(d.is_empty(), "null `then` value must be allowed: {d:?}");
    }

    #[test]
    fn flags_object_then_function_expression_value() {
        let d = run_on("const obj = { then: function() {} };");
        assert_eq!(d.len(), 1, "function-valued `then` must still flag: {d:?}");
        assert!(d[0].message.contains("object"));
    }

    #[test]
    fn flags_object_then_arrow_value() {
        let d = run_on("const obj = { then: () => {} };");
        assert_eq!(d.len(), 1, "arrow-valued `then` must still flag: {d:?}");
        assert!(d[0].message.contains("object"));
    }

    // ── Issue #3249: minimal thenable returned from a `Thenable`-typed fn ──

    #[test]
    fn allows_zustand_to_thenable_bridge() {
        // zustand `toThenable`: a sync-to-async bridge whose inner arrow declares
        // the return type `Thenable<Result>`. Both branches return a minimal
        // thenable (`then` with one param, `catch`, no `finally`) — intentional.
        let src = r#"
const toThenable =
  <Result, Input>(fn: (input: Input) => Result | Promise<Result> | Thenable<Result>) =>
  (input: Input): Thenable<Result> => {
    try {
      const result = fn(input);
      if (result instanceof Promise) {
        return result as Thenable<Result>;
      }
      return {
        then(onFulfilled) {
          return toThenable(onFulfilled)(result as Result);
        },
        catch(_onRejected) {
          return this as Thenable<any>;
        },
      };
    } catch (e: any) {
      return {
        then(_onFulfilled) {
          return this as Thenable<any>;
        },
        catch(onRejected) {
          return toThenable(onRejected)(e);
        },
      };
    }
  };
"#;
        let d = run_on(src);
        assert!(d.is_empty(), "Thenable-typed bridge must be allowed: {d:?}");
    }

    #[test]
    fn allows_minimal_thenable_returned_from_thenable_fn() {
        // Directly returned minimal thenable from a `Thenable`-typed arrow.
        let src = r#"
const make = (): Thenable<number> => {
  return {
    then(onFulfilled) { return onFulfilled(1); },
  };
};
"#;
        let d = run_on(src);
        assert!(d.is_empty(), "minimal thenable from Thenable fn must be allowed: {d:?}");
    }

    #[test]
    fn flags_then_object_returned_from_non_thenable_fn() {
        // Same minimal-thenable shape, but the function's return type is NOT
        // `Thenable` — no declaration of thenable intent, still an accidental
        // thenable.
        let src = r#"
const make = (): Foo => {
  return {
    then(onFulfilled) { return onFulfilled(1); },
  };
};
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1, "non-Thenable return type must still flag: {d:?}");
        assert!(d[0].message.contains("object"));
    }

    #[test]
    fn flags_incidental_then_object_not_returned() {
        // An ordinary config object with a `then` method, not returned from a
        // `Thenable`-typed function — still an accidental thenable.
        let src = r#"
const config = {
  then(onFulfilled) { return onFulfilled(); },
};
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1, "incidental `then` object must still flag: {d:?}");
        assert!(d[0].message.contains("object"));
    }

    #[test]
    fn flags_then_object_declared_but_not_returned_in_thenable_fn() {
        // The object is NOT in return position — it is bound to a local inside a
        // `Thenable`-typed function. The return-type intent applies to the
        // returned value, not to incidental objects, so this still flags.
        let src = r#"
const make = (): Thenable<number> => {
  const incidental = { then(onFulfilled) { return onFulfilled(1); } };
  use(incidental);
  return real;
};
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1, "declared-but-not-returned `then` must still flag: {d:?}");
        assert!(d[0].message.contains("object"));
    }

    #[test]
    fn flags_then_object_as_call_argument_in_thenable_fn() {
        // The object is an argument to `wrap(...)`, not the returned value, so
        // the `Thenable` return-type intent does not cover it.
        let src = r#"
const make = (): Thenable<number> => {
  return wrap({ then(onFulfilled) { return onFulfilled(1); } });
};
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1, "object as call argument must still flag: {d:?}");
        assert!(d[0].message.contains("object"));
    }

    #[test]
    fn flags_then_object_as_register_argument_in_thenable_fn() {
        let src = r#"
const make = (): Thenable<number> => {
  return register({ then(onFulfilled) { return onFulfilled(1); } });
};
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1, "object as register argument must still flag: {d:?}");
        assert!(d[0].message.contains("object"));
    }

    #[test]
    fn flags_then_object_nested_data_property_in_thenable_fn() {
        // The `then` is buried as a nested data property of the returned object,
        // not the returned value itself.
        let src = r#"
const make = (): Thenable<number> => {
  return { a: { b: { then(onFulfilled) { return onFulfilled(1); } } } };
};
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1, "nested-property `then` must still flag: {d:?}");
        assert!(d[0].message.contains("object"));
    }

    #[test]
    fn flags_then_object_as_array_element_in_thenable_fn() {
        // The object is an array element, not the returned value.
        let src = r#"
const make = (): Thenable<number> => {
  return [{ then(onFulfilled) { return onFulfilled(1); } }];
};
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1, "object as array element must still flag: {d:?}");
        assert!(d[0].message.contains("object"));
    }

    #[test]
    fn allows_minimal_thenable_arrow_expression_body() {
        // Implicit arrow return: `(): Thenable<T> => ({ then })`.
        let src = r#"
const make = (): Thenable<number> => ({
  then(onFulfilled) { return onFulfilled(1); },
});
"#;
        let d = run_on(src);
        assert!(d.is_empty(), "thenable as arrow expression body must be allowed: {d:?}");
    }

    #[test]
    fn allows_minimal_thenable_from_conditional_branch_return() {
        // The object is a ternary result branch of the returned value — a
        // transparent wrapper — so the `Thenable` intent still covers it.
        let src = r#"
const make = (flag: boolean): Thenable<number> => {
  return flag
    ? { then(onFulfilled) { return onFulfilled(1); } }
    : { then(onFulfilled) { return onFulfilled(2); } };
};
"#;
        let d = run_on(src);
        assert!(d.is_empty(), "conditional-branch return must be allowed: {d:?}");
    }

    #[test]
    fn allows_minimal_thenable_from_cast_return() {
        // The returned object is wrapped in an `as` cast — transparent.
        let src = r#"
const make = (): Thenable<number> => {
  return { then(onFulfilled) { return onFulfilled(1); } } as Thenable<number>;
};
"#;
        let d = run_on(src);
        assert!(d.is_empty(), "cast-wrapped return must be allowed: {d:?}");
    }

    #[test]
    fn allows_minimal_thenable_from_union_return_type() {
        // Union return type `T | Thenable<T>` still declares thenable intent.
        let src = r#"
function make(): number | Thenable<number> {
  return {
    then(onFulfilled) { return onFulfilled(1); },
  };
}
"#;
        let d = run_on(src);
        assert!(d.is_empty(), "union Thenable return type must be allowed: {d:?}");
    }
}
