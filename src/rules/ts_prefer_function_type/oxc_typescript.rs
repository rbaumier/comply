//! ts-prefer-function-type — OXC backend.
//! Flag interfaces / type literals with only a single call signature.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSSignature;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSInterfaceDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSInterfaceDeclaration(decl) = node.kind() else { return };

        // Skip interfaces that extend something other than `Function`.
        if !decl.extends.is_empty() {
            for heritage in &decl.extends {
                let span = heritage.expression.span();
                let name = &ctx.source[span.start as usize..span.end as usize];
                if name.trim() != "Function" {
                    return;
                }
            }
        }

        let members: Vec<_> = decl.body.body.iter().collect();
        if members.len() != 1 {
            return;
        }
        let sig = &members[0];
        let span = match sig {
            TSSignature::TSCallSignatureDeclaration(s) => {
                // Must have a return type.
                if s.return_type.is_none() {
                    return;
                }
                s.span
            }
            TSSignature::TSConstructSignatureDeclaration(s) => {
                if s.return_type.is_none() {
                    return;
                }
                s.span
            }
            _ => return,
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Interface only has a call signature — use a function type instead.".into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_interface_with_call_signature() {
        let diags = run_on("interface Fn { (): void; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Interface"));
    }

    #[test]
    fn allows_interface_with_multiple_members() {
        assert!(run_on("interface Foo { (): void; bar: number; }").is_empty());
    }

    #[test]
    fn allows_interface_extending_non_function() {
        assert!(run_on("interface Foo extends Bar { (): void; }").is_empty());
    }
}
