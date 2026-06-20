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

/// `localStorage.<method>(...)` — returns the call's first string-literal
/// argument when the callee matches, otherwise `None`.
fn localstorage_string_key<'a>(
    call: &'a CallExpression<'a>,
    method: &str,
) -> Option<&'a StringLiteral<'a>> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let Expression::Identifier(obj) = &member.object else {
        return None;
    };
    if obj.name != "localStorage" || member.property.name.as_str() != method {
        return None;
    }
    let Expression::StringLiteral(lit) = call.arguments.first()?.as_expression()? else {
        return None;
    };
    Some(lit)
}

/// The top-level `localStorage.<method>(...)` call of an expression statement.
fn statement_localstorage_call<'a>(
    stmt: &'a Statement<'a>,
    method: &str,
) -> Option<&'a StringLiteral<'a>> {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return None;
    };
    let Expression::CallExpression(call) = &expr_stmt.expression else {
        return None;
    };
    localstorage_string_key(call, method)
}

/// A `setItem(key, ...)` paired with a sibling `localStorage.removeItem(key)` in
/// the same block is a feature-detection probe (write-then-remove to test
/// `localStorage` availability), not persisted state, so it needs no version.
/// Scoping to the same block avoids exempting persisted keys that are merely
/// removed elsewhere (e.g. a logout cleanup path).
fn block_removes_key(stmts: &[Statement], key: &str) -> bool {
    stmts
        .iter()
        .any(|stmt| statement_localstorage_call(stmt, "removeItem").is_some_and(|lit| lit.value == key))
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
            let Some(lit) = statement_localstorage_call(stmt, "setItem") else {
                continue;
            };
            let key = lit.value.as_str();
            if has_version_suffix(key) || block_removes_key(stmts, key) {
                continue;
            }

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
                severity: Severity::Warning,
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
}
