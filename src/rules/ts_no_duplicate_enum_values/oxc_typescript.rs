//! ts-no-duplicate-enum-values oxc backend — flag duplicate values in enum declarations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Only string/number literals carry an accidental-copy-paste risk worth
/// flagging. References to another value (`= Other.Member`, `= SOME_CONST`)
/// that share a target are a deliberate N:1 mapping, not a bug, so they are
/// not enrolled in the duplicate check — matching ESLint `no-duplicate-enum-values`.
fn is_literal_initializer(expr: &Expression) -> bool {
    match expr {
        Expression::NumericLiteral(_) | Expression::StringLiteral(_) => true,
        Expression::UnaryExpression(u) => is_literal_initializer(&u.argument),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSEnumDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["enum"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSEnumDeclaration(decl) = node.kind() else { return };

        let mut seen: Vec<String> = Vec::new();
        for member in &decl.body.members {
            let Some(init) = &member.initializer else { continue };
            if !is_literal_initializer(init) {
                continue;
            }
            let init_span = init.span();
            let val = &ctx.source[init_span.start as usize..init_span.end as usize];
            let val = val.trim();
            if val.is_empty() {
                continue;
            }
            if seen.contains(&val.to_string()) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, init_span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Duplicate enum member value `{val}`."),
                    severity: Severity::Warning,
                    span: None,
                });
            } else {
                seen.push(val.to_string());
            }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_duplicate_number_literals() {
        let d = run("enum E { A = 1, B = 1 }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Duplicate"));
    }

    #[test]
    fn flags_duplicate_string_literals() {
        let d = run(r#"enum E { A = "x", B = "x" }"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_unique_values() {
        assert!(run("enum E { A = 1, B = 2 }").is_empty());
    }

    /// Regression #1136: an N:1 mapping enum where members reference the same
    /// other enum member (`= AmqpResponseStatusCode.Gone`) is a deliberate
    /// alias, not a copy-paste bug — must not be flagged.
    #[test]
    fn allows_duplicate_member_references() {
        let src = r#"
            enum E {
                "a" = Status.Gone,
                "b" = Status.Gone,
                "c" = Status.Gone,
                "d" = Status.BadRequest,
                "e" = Status.BadRequest,
            }
        "#;
        assert!(run(src).is_empty());
    }

    /// Negative-space guard: two independent literal members with the same
    /// value remain the bug-prone shape and stay flagged.
    #[test]
    fn still_flags_duplicate_literals_alongside_references() {
        let src = r#"
            enum E {
                "a" = Status.Gone,
                "b" = Status.Gone,
                X = 1,
                Y = 1,
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
