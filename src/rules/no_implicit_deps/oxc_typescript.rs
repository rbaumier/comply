//! no-implicit-deps oxc backend — flag bare `import` specifiers that are not
//! declared in the nearest ancestor `package.json` and are not Node.js
//! builtins.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use super::{
    is_bare_specifier, is_node_builtin, is_virtual_module, matches_alias, root_package_name,
};

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

        // Stay silent if there's no `package.json` anywhere above this file.
        let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
            return;
        };
        let alias_prefixes = ctx
            .project
            .nearest_tsconfig(ctx.path)
            .map(|t| t.alias_prefixes())
            .unwrap_or_default();

        let spec = import.source.value.as_str();

        if !is_bare_specifier(spec) {
            return;
        }
        if is_node_builtin(spec) {
            return;
        }
        if is_virtual_module(spec) {
            return;
        }
        if matches_alias(spec, &alias_prefixes) {
            return;
        }
        let root = root_package_name(spec);
        if pkg.has_dep_or_engine(root) {
            return;
        }
        // Workspace fallback: check workspace package names and the root
        // package.json when the nearest manifest doesn't list the dep.
        if ctx
            .project
            .workspace_package_names()
            .iter()
            .any(|n| n == root)
        {
            return;
        }
        if let Some(root_pkg) = &ctx.project.package_json {
            if root_pkg.has_dep_or_engine(root) {
                return;
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Bare import `{spec}` is not listed in package.json (checked root `{root}`)."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
