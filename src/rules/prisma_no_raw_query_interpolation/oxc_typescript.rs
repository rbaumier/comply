//! OxcCheck backend — flag `<x>.$queryRaw(...)` and `<x>.$executeRaw(...)` call forms.
//! The safe form is the tagged template literal `<x>.$queryRaw\`...\``.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client")
        || crate::oxc_helpers::source_contains(source, "PrismaClient")
        || crate::oxc_helpers::source_contains(source, "$queryRaw")
        || crate::oxc_helpers::source_contains(source, "$executeRaw")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["$queryRaw", "$executeRaw"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_prisma_file(ctx.source) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let prop_text = member.property.name.as_str();
        if !matches!(prop_text, "$queryRaw" | "$executeRaw") {
            return;
        }

        // The tagged-template form `prisma.$queryRaw\`...\`` is parsed by oxc
        // as a TaggedTemplateExpression, not a CallExpression — so if we get
        // here it's the unsafe call form.

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{prop_text}(...)` accepts a string — concatenated input is SQL injection. \
                 Use the tagged-template form: `prisma.{prop_text}\\`SELECT ...\\``."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
