//! ts-no-namespace oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSModuleDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["namespace"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSModuleDeclaration(decl) = node.kind() else { return };

        // Allow `declare namespace` (ambient declarations).
        if decl.declare {
            return;
        }

        // Only flag `namespace`, not `module`.
        let text = &ctx.source[decl.span.start as usize..decl.span.end as usize];
        if !text.starts_with("namespace") && !text.starts_with("export namespace") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "TypeScript `namespace` is a legacy construct \u{2014} \
                      use ES module `export` / `import` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
