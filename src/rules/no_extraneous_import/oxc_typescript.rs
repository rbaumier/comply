//! no-extraneous-import OXC backend.
//!
//! Flags imports of devDependency packages from non-test production files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::path::Path;
use std::sync::Arc;

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

        let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
            return;
        };
        if is_test_file(ctx.path) {
            return;
        }
        if crate::rules::path_utils::is_config_file(ctx.path) {
            return;
        }

        let specifier = import.source.value.as_str();
        if !is_bare_specifier(specifier) {
            return;
        }

        let root = package_root(specifier);
        let in_runtime = pkg.dependencies.contains_key(root)
            || pkg.peer_dependencies.contains_key(root)
            || pkg.optional_dependencies.contains_key(root);
        if in_runtime {
            return;
        }

        if pkg.dev_dependencies.contains_key(root) {
            let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "no-extraneous-import".into(),
                message: format!(
                    "`{root}` is a devDependency; production code should import from dependencies, peerDependencies, or optionalDependencies."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn package_root(specifier: &str) -> &str {
    if specifier.starts_with('@') {
        match specifier.find('/') {
            Some(first_slash) => match specifier[first_slash + 1..].find('/') {
                Some(second_slash) => &specifier[..first_slash + 1 + second_slash],
                None => specifier,
            },
            None => specifier,
        }
    } else {
        match specifier.find('/') {
            Some(slash) => &specifier[..slash],
            None => specifier,
        }
    }
}

fn is_test_file(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.contains("__tests__")
        || path_str.contains(".test.")
        || path_str.contains(".spec.")
        || path_str.contains(".stories.")
        || path_str.contains(".setup.")
        || path_str.contains("/test/")
        || path_str.contains("/tests/")
        || path_str.contains("/e2e/")
}

fn is_bare_specifier(spec: &str) -> bool {
    !spec.is_empty()
        && !spec.starts_with('.')
        && !spec.starts_with('/')
        && !spec.starts_with("node:")
}
