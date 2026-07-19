//! no-unchecked-json-parse OXC backend — flag unwrapped `JSON.parse(...)` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_json_method_call};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// True when `id` resolves to a binding declared with an explicit `unknown`
/// type annotation (e.g. `let body: unknown`). Assigning `JSON.parse()`'s
/// `any` result to an `unknown` target is safe: TypeScript forces every
/// downstream consumer to narrow before use — the same guarantee the rule
/// enforces. A concrete annotation is *not* exempt: `any → T` is a silent
/// unsafe assertion, exactly what the rule should still flag.
fn binding_is_unknown_typed<'a>(
    id: &oxc_ast::ast::IdentifierReference<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ref_id) = id.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id)) {
        if let AstKind::VariableDeclarator(decl) = kind {
            return is_unknown_annotation(decl.type_annotation.as_deref());
        }
    }
    false
}

/// True when the annotation is exactly `unknown`.
fn is_unknown_annotation(ann: Option<&oxc_ast::ast::TSTypeAnnotation<'_>>) -> bool {
    matches!(
        ann.map(|a| &a.type_annotation),
        Some(oxc_ast::ast::TSType::TSUnknownKeyword(_))
    )
}

/// True when `inner` is fully contained within `outer`.
fn span_contains(outer: oxc_span::Span, inner: oxc_span::Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// True when the call sits inside the `try` block of a `try { … } catch { … }`
/// statement within its own function. The catch handler turns a throwing
/// `JSON.parse` into a recoverable failure (the caller returns a fallback or
/// rethrows), which is the protection this rule asks for — exactly how a
/// safe-parse wrapper (e.g. `destr`) guards its own `JSON.parse`. The walk stops
/// at the enclosing function boundary: a `try/catch` in an outer function does
/// not guard a `JSON.parse` that throws across the call stack.
fn is_in_guarded_try<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    use oxc_span::GetSpan;

    let call_span = node.kind().span();
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::TryStatement(try_stmt) => {
                if try_stmt.handler.is_some() && span_contains(try_stmt.block.span, call_span) {
                    return true;
                }
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["JSON"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `JSON.parse`.
        if !is_json_method_call(call, "parse") {
            return;
        }

        // Skip the deep-clone idiom `JSON.parse(JSON.stringify(x))`: the parsed
        // value is a re-serialization of a value the program already holds and
        // types, not untrusted external input. There is no unknown shape to
        // validate — schema-guarding a clone of one's own typed value is noise.
        // Only the direct `JSON.stringify(...)` argument shape qualifies; the
        // one-hop `const s = JSON.stringify(x); JSON.parse(s)` is out of scope.
        if let Some(Expression::CallExpression(arg_call)) =
            call.arguments.first().and_then(|arg| arg.as_expression())
            && is_json_method_call(arg_call, "stringify")
        {
            return;
        }

        // Check if wrapped in a validator: parent is an argument to .parse()/.safeParse().
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::CallExpression(outer_call) = parent.kind()
            && let Expression::StaticMemberExpression(outer_member) = &outer_call.callee {
                let method = outer_member.property.name.as_str();
                if method == "parse" || method == "safeParse" {
                    return;
                }
            }

        // `JSON.parse()` assigned to an `unknown`-typed target is safe (#512):
        // the result cannot be used without runtime narrowing.
        match parent.kind() {
            AstKind::AssignmentExpression(assign) => {
                if let oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left
                    && binding_is_unknown_typed(id, semantic)
                {
                    return;
                }
            }
            AstKind::VariableDeclarator(decl) => {
                if is_unknown_annotation(decl.type_annotation.as_deref()) {
                    return;
                }
            }
            _ => {}
        }

        // A `JSON.parse` inside a `try { … } catch { … }` is guarded: the catch
        // handler intercepts the throw, so the result is never consumed on a
        // parse failure (#5251).
        if is_in_guarded_try(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`JSON.parse()` returns `any` — wrap it with a Zod schema or type guard before using the result.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
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
    use super::Check;
    
    #[test]
    fn allows_json_parse_assigned_to_unknown_issue_512() {
        let src = r#"
            let body: unknown = null;
            if (text.length > 0) {
                try {
                    body = JSON.parse(text);
                } catch {
                    body = text;
                }
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_json_parse_in_unknown_declarator() {
        let src = "const body: unknown = JSON.parse(text);";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn still_flags_unwrapped_json_parse() {
        let src = "const data = JSON.parse(text);";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_json_parse_assigned_to_concrete_type() {
        let src = r#"
            let cfg: Config = defaultConfig;
            cfg = JSON.parse(text);
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_json_parse_returned_from_try_with_catch_issue_5251() {
        // destr's safe-parse shape: JSON.parse inside a try whose catch returns
        // a fallback. Both call sites (one nested in an `if`) are guarded.
        let src = r#"
            export function destr<T = unknown>(value: any, options: Options = {}): T {
                try {
                    if (suspectProtoRx.test(value)) {
                        if (options.strict) {
                            throw new Error("[destr] Possible prototype pollution");
                        }
                        return JSON.parse(value, jsonParseTransform);
                    }
                    return JSON.parse(value);
                } catch (error) {
                    if (options.strict) throw error;
                    return value as T;
                }
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn still_flags_unwrapped_json_parse_with_no_try() {
        let src = "const input = readFile(); const x = JSON.parse(input);";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_json_parse_in_try_without_catch() {
        // A try/finally with no catch handler does not intercept the throw,
        // so the parse result is not guarded.
        let src = r#"
            function load() {
                try {
                    const cfg = JSON.parse(text);
                    return cfg;
                } finally {
                    cleanup();
                }
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_json_parse_in_inner_function_of_outer_try() {
        // The outer try/catch cannot catch a throw that escapes the inner
        // function across the call stack, so the inner parse is still flagged.
        let src = r#"
            function outer() {
                try {
                    const parse = () => {
                        const cfg = JSON.parse(text);
                        return cfg;
                    };
                    return parse;
                } catch {
                    return null;
                }
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn skips_json_parse_of_json_stringify_round_trip_issue_7561() {
        // #7561: `JSON.parse(JSON.stringify(obj))` re-serializes a value the
        // program already holds and types — a deep clone / undefined-strip, not
        // an untrusted-input boundary. There is no unknown shape to validate.
        let src = "const clone = JSON.parse(JSON.stringify(obj));";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn skips_json_parse_of_json_stringify_with_as_cast_issue_7561() {
        // The issue's `removeUndefined` shape: the `as T` cast wraps the parse
        // call, its `JSON.stringify` argument is unchanged, so it stays silent.
        let src = "const removeUndefined = <T extends Record<string, unknown>>(obj: T): T => JSON.parse(JSON.stringify(obj)) as T;";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn still_flags_json_parse_of_non_json_stringify() {
        // The skip is specific to `JSON.stringify`: a `stringify` method on any
        // other object gives no provenance guarantee, so it stays flagged.
        let src = "const data = JSON.parse(notJson.stringify(x));";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_json_parse_in_catch_clause() {
        // A parse in the catch body is not inside the try block, so the
        // handler does not guard it.
        let src = r#"
            function load() {
                try {
                    risky();
                } catch {
                    const cfg = JSON.parse(text);
                    return cfg;
                }
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }
}
