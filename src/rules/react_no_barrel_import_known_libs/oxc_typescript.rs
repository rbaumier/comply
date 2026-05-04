//! OXC backend for react-no-barrel-import-known-libs.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const BARREL_LIBS: &[&str] = &["@mui/material", "@mui/icons-material", "lodash", "date-fns"];

const TREE_SHAKEABLE_ALLOWLIST: &[&str] = &[
    "lucide-react",
    "@heroicons/react/*",
    "@phosphor-icons/react",
    "react-icons/*",
];

fn matches_allowlist(source: &str) -> bool {
    TREE_SHAKEABLE_ALLOWLIST
        .iter()
        .any(|pat| match pat.strip_suffix('*') {
            Some(prefix) => source.starts_with(prefix),
            None => source == *pat,
        })
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
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };

        // Must have named specifiers (not just default/namespace)
        let Some(specifiers) = &import.specifiers else {
            return;
        };
        let has_named = specifiers.iter().any(|s| {
            matches!(s, oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(_))
        });
        if !has_named {
            return;
        }

        let import_path = import.source.value.as_str();
        if matches_allowlist(import_path) {
            return;
        }
        if !BARREL_LIBS.contains(&import_path) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.source.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Named import from `{import_path}` pulls the entire barrel — \
                 import from a subpath (e.g. `{import_path}/<name>`) for \
                 tree-shaking."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
