//! require-path-exists OxcCheck backend — flag imports pointing to non-existent files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::path::Path;
use std::sync::Arc;

const EXTENSIONS: &[&str] = &[
    "",
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".mjs",
    ".cjs",
    ".json",
    "/index.ts",
    "/index.tsx",
    "/index.js",
    "/index.jsx",
    "/index.mjs",
];

fn is_relative_path(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../")
}

fn resolve_and_check(base_dir: &Path, import_spec: &str) -> bool {
    let resolved = base_dir.join(import_spec);

    for ext in EXTENSIONS {
        let candidate = if ext.is_empty() {
            resolved.clone()
        } else if let Some(dir_ext) = ext.strip_prefix('/') {
            resolved.join(dir_ext)
        } else if let Some(file_ext) = ext.strip_prefix('.') {
            resolved.with_extension(file_ext)
        } else {
            continue;
        };

        if candidate.exists() {
            return true;
        }
    }

    let with_ts = format!("{}.ts", resolved.display());
    let with_tsx = format!("{}.tsx", resolved.display());
    Path::new(&with_ts).exists() || Path::new(&with_tsx).exists()
}

fn extract_spec_from_string(source: &str, span: oxc_span::Span) -> &str {
    let raw = &source[span.start as usize..span.end as usize];
    raw.trim_matches(|c| c == '\'' || c == '"' || c == '`')
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::ImportDeclaration,
            AstType::ExportNamedDeclaration,
            AstType::ExportDefaultDeclaration,
            AstType::CallExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let import_spec = match node.kind() {
            AstKind::ImportDeclaration(decl) => {
                extract_spec_from_string(ctx.source, decl.source.span).to_string()
            }
            AstKind::ExportNamedDeclaration(decl) => {
                let Some(ref src) = decl.source else { return };
                extract_spec_from_string(ctx.source, src.span).to_string()
            }
            AstKind::ExportDefaultDeclaration(_) => return,
            AstKind::CallExpression(call) => {
                // require("...")
                let is_require = match &call.callee {
                    oxc_ast::ast::Expression::Identifier(id) => id.name == "require",
                    _ => false,
                };
                if !is_require {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else { return };
                let oxc_ast::ast::Argument::StringLiteral(lit) = first_arg else { return };
                lit.value.to_string()
            }
            _ => return,
        };

        if !is_relative_path(&import_spec) {
            return;
        }

        let Some(base_dir) = ctx.path.parent() else { return };

        if !resolve_and_check(base_dir, &import_spec)
            && !crate::rules::path_utils::is_relative_specifier_gitignored(base_dir, &import_spec)
        {
            let span = match node.kind() {
                AstKind::ImportDeclaration(d) => d.span,
                AstKind::ExportNamedDeclaration(d) => d.span,
                AstKind::CallExpression(c) => c.span,
                _ => return,
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("Import path '{import_spec}' does not exist."),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
