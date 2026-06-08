//! empty-brace-spaces oxc backend — flag `{ }`, `{  }` (spaces inside empty braces).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::ObjectExpression,
            AstType::BlockStatement,
            AstType::Class,
            AstType::ObjectPattern,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let span = match node.kind() {
            AstKind::ObjectExpression(obj) => {
                if !obj.properties.is_empty() {
                    return;
                }
                obj.span
            }
            AstKind::BlockStatement(block) => {
                if !block.body.is_empty() {
                    return;
                }
                block.span
            }
            AstKind::Class(class) => {
                if !class.body.body.is_empty() {
                    return;
                }
                class.body.span
            }
            AstKind::ObjectPattern(pat) => {
                if !pat.properties.is_empty() || pat.rest.is_some() {
                    return;
                }
                pat.span
            }
            _ => return,
        };

        let text = &ctx.source[span.start as usize..span.end as usize];

        // Must be `{ ... }` with only whitespace inside.
        if !text.starts_with('{') || !text.ends_with('}') {
            return;
        }

        let inner = &text[1..text.len() - 1];
        if inner.is_empty() {
            return; // `{}` is fine
        }

        if !inner.chars().all(|c| c.is_whitespace()) {
            return; // has content
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Do not add spaces between braces: `{text}` -> `{{}}`.",),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_single_space() {
        let d = run_on("const obj = { };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("{}"));
    }


    #[test]
    fn flags_multiple_spaces() {
        let d = run_on("class Foo {   }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_empty_braces_no_space() {
        assert!(run_on("const obj = {};").is_empty());
    }


    #[test]
    fn allows_braces_with_content() {
        assert!(run_on("const obj = { a: 1 };").is_empty());
    }
}
