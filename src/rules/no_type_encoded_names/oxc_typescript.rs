//! no-type-encoded-names — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator, AstType::FormalParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let name = match node.kind() {
            oxc_ast::AstKind::VariableDeclarator(decl) => {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(ref id) = decl.id {
                    (&*id.name, id.span())
                } else {
                    return;
                }
            }
            oxc_ast::AstKind::FormalParameter(param) => {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(ref id) = param.pattern {
                    (&*id.name, id.span())
                } else {
                    return;
                }
            }
            _ => return,
        };

        let (ident, span) = name;
        let Some(prefix) = super::type_prefix::matched_camel_case(ident) else {
            return;
        };
        let (line, col) = byte_offset_to_line_col(semantic.source_text(), span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: format!(
                "'{ident}' encodes a type prefix '{prefix}' — Hungarian notation is \
                 obsolete. Remove the prefix; TypeScript's type checker already \
                 knows the type."
            ),
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
    fn flags_camel_case_hungarian() {
        assert_eq!(run("const strValue = 'x';").len(), 1);
    }

    // Regression for #279: SCREAMING_SNAKE domain constants are not Hungarian.
    #[test]
    fn allows_screaming_snake_domain_constants() {
        assert!(run("const PROMPTS_DIR = '/p';").is_empty());
        assert!(run("const PROMPT_FILE = 'p.txt';").is_empty());
    }
}
