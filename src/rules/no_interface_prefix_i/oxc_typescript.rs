use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
        let AstKind::TSInterfaceDeclaration(iface) = node.kind() else {
            return;
        };

        let name = iface.id.name.as_str();
        let bytes = name.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'I' || !bytes[1].is_ascii_uppercase() {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, iface.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Interface `{name}` uses the `I` prefix — rename to `{}`.",
                &name[1..]
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
