//! react-require-versioned-storage-key oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{CallExpression, Expression, Statement, StringLiteral};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn has_version_suffix(key: &str) -> bool {
    let Some(idx) = key.rfind(":v") else {
        return false;
    };
    let suffix = &key[idx + 2..];
    !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
}

/// The top-level `localStorage.<method>(...)` call of an expression statement,
/// when the callee is exactly `localStorage.<method>`.
fn statement_localstorage_call<'a>(
    stmt: &'a Statement<'a>,
    method: &str,
) -> Option<&'a CallExpression<'a>> {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return None;
    };
    let Expression::CallExpression(call) = &expr_stmt.expression else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let Expression::Identifier(obj) = &member.object else {
        return None;
    };
    if obj.name != "localStorage" || member.property.name.as_str() != method {
        return None;
    }
    Some(call)
}

/// The call's first argument when it is a string literal (the storage key).
fn call_string_key<'a>(call: &'a CallExpression<'a>) -> Option<&'a StringLiteral<'a>> {
    let Expression::StringLiteral(lit) = call.arguments.first()?.as_expression()? else {
        return None;
    };
    Some(lit)
}

/// A `JSON.stringify` static-member callee.
fn is_json_stringify(callee: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    obj.name == "JSON" && member.property.name.as_str() == "stringify"
}

/// The stored value carries a *serialized shape* that can change across
/// versions — `JSON.stringify(...)`, an object literal, or an array literal.
/// A scalar (`string`/`number`/`boolean`) or any other expression has no shape
/// to migrate, so a `:vN` suffix buys nothing.
fn value_has_migratable_shape(value: &Expression) -> bool {
    match value.without_parentheses() {
        Expression::ObjectExpression(_) | Expression::ArrayExpression(_) => true,
        Expression::CallExpression(call) => is_json_stringify(&call.callee),
        _ => false,
    }
}

/// A `setItem(key, ...)` paired with a sibling `localStorage.removeItem(key)` in
/// the same block is a feature-detection probe (write-then-remove to test
/// `localStorage` availability), not persisted state, so it needs no version.
/// Scoping to the same block avoids exempting persisted keys that are merely
/// removed elsewhere (e.g. a logout cleanup path).
fn block_removes_key(stmts: &[Statement], key: &str) -> bool {
    stmts.iter().any(|stmt| {
        statement_localstorage_call(stmt, "removeItem")
            .and_then(call_string_key)
            .is_some_and(|lit| lit.value == key)
    })
}

/// The unversioned storage key of a `localStorage.setItem(key, value)` statement
/// that warrants a `:vN` suffix: a literal key with none, that is not a
/// feature-detection probe, whose value carries a migratable serialized shape.
fn flaggable_setitem_key<'a>(
    stmt: &'a Statement<'a>,
    block: &'a [Statement<'a>],
) -> Option<&'a StringLiteral<'a>> {
    let call = statement_localstorage_call(stmt, "setItem")?;
    let key_lit = call_string_key(call)?;
    let key = key_lit.value.as_str();
    if has_version_suffix(key) || block_removes_key(block, key) {
        return None;
    }
    let value = call.arguments.get(1)?.as_expression()?;
    if !value_has_migratable_shape(value) {
        return None;
    }
    Some(key_lit)
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["localStorage"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Program, AstType::BlockStatement, AstType::FunctionBody]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let stmts: &[Statement] = match node.kind() {
            AstKind::Program(prog) => &prog.body,
            AstKind::BlockStatement(block) => &block.body,
            AstKind::FunctionBody(body) => &body.statements,
            _ => return,
        };

        for stmt in stmts {
            let Some(lit) = flaggable_setitem_key(stmt, stmts) else {
                continue;
            };
            let key = lit.value.as_str();

            let (line, column) = byte_offset_to_line_col(ctx.source, lit.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Storage key `{key}` has no `:vN` version suffix \u{2014} bumping the \
                     version lets you migrate or drop old entries when the shape changes."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
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
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_unversioned_key() {
        let src = r#"localStorage.setItem("settings", JSON.stringify(x));"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_versioned_key() {
        let src = r#"localStorage.setItem("settings:v1", JSON.stringify(x));"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_dynamic_key() {
        let src = r#"localStorage.setItem(key, "v");"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_feature_detection_probe() {
        // verdaccio packages/ui-components/src/store/storage.ts — write/remove
        // probe to test localStorage availability is a throwaway, not state.
        let src = r#"
            let storage;
            try {
              localStorage.setItem("__TEST__", "");
              localStorage.removeItem("__TEST__");
              storage = localStorage;
            } catch {
              storage = memoryStorage;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_persisted_key_even_with_unrelated_remove() {
        // A genuine persisted key is still flagged; the probe exemption only
        // applies when the SAME key is removed in the SAME block.
        let src = r#"
            localStorage.setItem("settings", JSON.stringify(x));
            localStorage.removeItem("__TEST__");
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_persisted_key_removed_in_a_different_block() {
        // Persisted state set on load and cleared on logout (a normal lifecycle)
        // is not a probe — the set and remove live in different blocks.
        let src = r#"
            function init() {
              localStorage.setItem("session", JSON.stringify(x));
            }
            function logout() {
              localStorage.removeItem("session");
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_probe_inside_function_body() {
        let src = r#"
            function available() {
              localStorage.setItem("__probe__", "");
              localStorage.removeItem("__probe__");
              return true;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_scalar_string_value() {
        // lin-xin/vue-manage-system src/store/sidebar.ts — a Pinia action persists
        // a plain string (a hex color); a scalar value has no serialized shape to
        // version, so the `:vN` suffix would buy nothing.
        let src = r#"
            const store = {
                actions: {
                    setBgColor(color: string) {
                        this.bgColor = color;
                        localStorage.setItem('sidebar-bg-color', color);
                    },
                    setTextColor(color: string) {
                        this.textColor = color;
                        localStorage.setItem('sidebar-text-color', color);
                    }
                }
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_scalar_literal_value() {
        let src = r#"localStorage.setItem("theme", "dark");"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_object_literal_value() {
        let src = r#"localStorage.setItem("settings", { theme: "dark" });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_array_literal_value() {
        let src = r#"localStorage.setItem("history", ["a", "b"]);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_parenthesized_stringify_value() {
        let src = r#"localStorage.setItem("settings", (JSON.stringify(x)));"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_non_stringify_call_value() {
        // Only `JSON.stringify(...)` is a serialized shape; an arbitrary call has
        // an unknown shape, so it is left unflagged (an accepted false negative,
        // consistent with the rule ignoring dynamic keys).
        let src = r#"localStorage.setItem("token", getToken());"#;
        assert!(run(src).is_empty());
    }
}
