use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "__mocks__"];

fn is_test_path(path: &str) -> bool {
    TEST_MARKERS.iter().any(|m| path.contains(m))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let current_path = ctx.path.to_string_lossy();
        if is_test_path(&current_path) {
            return;
        }
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let module = import.source.value.as_str();
        if !is_test_path(module) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        let range_start = import.span.start as usize;
        let range_len = (import.span.end - import.span.start) as usize;
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Production file imports test/mock module `{module}` \u{2014} move shared helpers out of the test file."
            ),
            severity: Severity::Warning,
            span: Some((range_start, range_len)),
        });
    }
}
