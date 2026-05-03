use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn file_is_layout(path: &std::path::Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem == "_layout")
}

fn dir_has_layout(dir: &std::path::Path) -> bool {
    let Ok(read) = std::fs::read_dir(dir) else {
        return true;
    };
    for entry in read.flatten() {
        let p = entry.path();
        if file_is_layout(&p) {
            return true;
        }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["expo-router"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        if import.source.value.as_str() != "expo-router" {
            return;
        }
        if file_is_layout(ctx.path) {
            return;
        }
        let Some(dir) = ctx.path.parent() else {
            return;
        };
        if dir_has_layout(dir) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Directory imports `expo-router` but is missing a `_layout` file.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
