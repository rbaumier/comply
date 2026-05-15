//! ts-no-mixed-enums oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSEnumDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSEnumDeclaration(enum_decl) = node.kind() else {
            return;
        };
        let mut has_string = false;
        let mut has_numeric = false;
        for member in &enum_decl.body.members {
            let Some(initializer) = &member.initializer else {
                // No initializer → numeric (sequential).
                has_numeric = true;
                continue;
            };
            match initializer {
                Expression::StringLiteral(_)
                | Expression::TemplateLiteral(_) => has_string = true,
                Expression::NumericLiteral(_) => has_numeric = true,
                Expression::UnaryExpression(u) => {
                    if matches!(u.argument, Expression::NumericLiteral(_)) {
                        has_numeric = true;
                    }
                }
                _ => {}
            }
        }
        if !(has_string && has_numeric) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, enum_decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Enum has both numeric and string members — pick one shape per \
                      enum to keep inference and serialization predictable."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_mixed_enum() {
        let src = r#"enum E { A = 1, B = "two" }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_all_numeric() {
        let src = r#"enum E { A, B, C = 5 }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_all_string() {
        let src = r#"enum E { A = "a", B = "b" }"#;
        assert!(run(src).is_empty());
    }
}
