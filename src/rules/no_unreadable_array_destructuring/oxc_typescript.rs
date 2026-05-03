//! no-unreadable-array-destructuring oxc backend.
//!
//! Flag destructuring patterns with consecutive holes (commas without
//! elements). Uses source text heuristic — same approach as the
//! tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclaration(decl) = node.kind() else {
            return;
        };

        for declarator in &decl.declarations {
            let oxc_ast::ast::BindingPattern::ArrayPattern(arr) = &declarator.id else {
                continue;
            };

            let src =
                &ctx.source[arr.span.start as usize..arr.span.end as usize];

            // Quick heuristic: if no ",," exists, skip.
            if !src.contains(",,") {
                continue;
            }

            // Count commas vs total slots.
            let comma_count = src.chars().filter(|&c| c == ',').count();
            let total_slots = comma_count + 1;
            if total_slots < 3 {
                continue;
            }

            // Confirm consecutive empty slots at the array pattern level
            // (not inside nested structures).
            let bytes = src.as_bytes();
            let mut depth = 0i32;
            let mut prev_was_comma = false;
            let mut found_consecutive = false;

            for &b in bytes.iter() {
                match b {
                    b'[' | b'(' | b'{' => {
                        depth += 1;
                        prev_was_comma = false;
                    }
                    b']' | b')' | b'}' => {
                        depth -= 1;
                        prev_was_comma = false;
                    }
                    b',' if depth == 1 => {
                        if prev_was_comma {
                            found_consecutive = true;
                            break;
                        }
                        prev_was_comma = true;
                    }
                    b' ' | b'\t' | b'\n' | b'\r' => {}
                    _ => {
                        prev_was_comma = false;
                    }
                }
            }

            if !found_consecutive {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, arr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Array destructuring may not contain consecutive ignored values."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_consecutive_holes_leading() {
        let d = run_on("const [,, third] = arr;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_many_consecutive_holes() {
        let d = run_on("const [,, third,,,, seventh] = arr;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_middle_consecutive_holes() {
        let d = run_on("const [first,,, fourth] = arr;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_single_holes() {
        assert!(run_on("const [a, , b] = arr;").is_empty());
    }

    #[test]
    fn allows_simple_destructuring() {
        assert!(run_on("const [a, b, c] = arr;").is_empty());
    }

    #[test]
    fn allows_single_element() {
        assert!(run_on("const [a] = arr;").is_empty());
    }
}
