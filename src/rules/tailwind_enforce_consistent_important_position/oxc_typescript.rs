//! tailwind-enforce-consistent-important-position oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn has_suffix_important(token: &str) -> bool {
    // After the last variant separator `:`, the class must NOT end in `!`.
    let class = token.rsplit(':').next().unwrap_or(token);
    // Bare `!` doesn't count.
    class.len() > 1 && class.ends_with('!')
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["className", "class"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else {
            return;
        };
        let oxc_ast::ast::JSXAttributeName::Identifier(ident) = &attr.name else {
            return;
        };
        let name = ident.name.as_str();
        if name != "className" && name != "class" {
            return;
        }
        let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
            return;
        };
        for token in lit.value.as_str().split_whitespace() {
            if has_suffix_important(token) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Suffix `{token}` uses the trailing `!` form — Tailwind v4 \
                         documents the prefix form (`!class`). Use it everywhere for \
                         consistency."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(src, &Check)
    }

    #[test]
    fn flags_suffix_important() {
        let src = r#"const x = <div className="text-red-500!" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_prefix_important() {
        let src = r#"const x = <div className="!text-red-500" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_important() {
        let src = r#"const x = <div className="text-red-500 mt-4" />;"#;
        assert!(run(src).is_empty());
    }
}
