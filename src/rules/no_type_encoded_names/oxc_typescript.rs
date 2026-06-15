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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
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

    // Regression for #3371: a single all-caps word naming a format (BYTE = the
    // base64 regex) is not Hungarian for `byt` + `E`.
    #[test]
    fn allows_single_all_caps_word_constant() {
        assert!(run("const BYTE = /^x$/gm;").is_empty());
        assert!(run("const STRING = /^x$/;").is_empty());
        // Genuine Hungarian still flags.
        assert_eq!(run("const bytValue = 1;").len(), 1);
    }
}
