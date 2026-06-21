//! prefer-import-meta-properties OXC backend.
//!
//! Flags `fileURLToPath(import.meta.url)` and
//! `dirname(fileURLToPath(import.meta.url))` patterns, but only when the called
//! identifiers resolve to the matching Node core module: `fileURLToPath` from
//! `node:url`/`url` and `dirname` from `node:path`/`path`. Same-named functions
//! imported from third-party packages or defined locally are left alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, import_root_package};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

const URL_MODULES: &[&str] = &["node:url", "url"];
const PATH_MODULES: &[&str] = &["node:path", "path"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["import.meta"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !super::engines_allow_import_meta_dirname(ctx) {
            return;
        }
        let source = ctx.source;

        // 1. `path.dirname(fileURLToPath(import.meta.url))`
        if is_method_call_with_import_meta_url(call, "path", "dirname", source)
            && namespace_is_node_path(call, semantic)
            && inner_filename_call_resolves_to_node_url(call, semantic)
        {
            let (line, column) = byte_offset_to_line_col(source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "prefer-import-meta-properties".into(),
                message: "Use `import.meta.dirname` instead of `path.dirname(fileURLToPath(import.meta.url))`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // 2. `dirname(fileURLToPath(import.meta.url))`
        if is_call_to_with_import_meta_url_two_levels(call, "dirname", "fileURLToPath", source)
            && is_imported_from(call_callee_name(call), PATH_MODULES, semantic)
            && inner_filename_call_resolves_to_node_url(call, semantic)
        {
            let (line, column) = byte_offset_to_line_col(source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "prefer-import-meta-properties".into(),
                message: "Use `import.meta.dirname` instead of `dirname(fileURLToPath(import.meta.url))`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // 3. `fileURLToPath(import.meta.url)` — skip if parent is dirname wrapper
        if is_call_to_with_import_meta_url(call, "fileURLToPath", source)
            && is_imported_from(call_callee_name(call), URL_MODULES, semantic)
        {
            if has_dirname_wrapper_parent(node, semantic, source) {
                return;
            }
            let (line, column) = byte_offset_to_line_col(source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "prefer-import-meta-properties".into(),
                message: "Use `import.meta.filename` instead of `fileURLToPath(import.meta.url)`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn is_import_meta_url(expr: &Expression, source: &str) -> bool {
    let Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    if member.property.name.as_str() != "url" {
        return false;
    }
    // The object should be `import.meta` — a MetaProperty
    let Expression::MetaProperty(_) = &member.object else {
        // Fallback: check source text
        let obj_text = &source[member.object.span().start as usize..member.object.span().end as usize];
        return obj_text == "import.meta";
    };
    true
}

fn single_call_arg<'a>(call: &'a CallExpression<'a>) -> Option<&'a Expression<'a>> {
    if call.arguments.len() != 1 {
        return None;
    }
    call.arguments[0].as_expression()
}

fn is_call_to_with_import_meta_url(
    call: &CallExpression,
    expected_callee: &str,
    source: &str,
) -> bool {
    let Expression::Identifier(id) = &call.callee else {
        return false;
    };
    if id.name.as_str() != expected_callee {
        return false;
    }
    let Some(arg) = single_call_arg(call) else {
        return false;
    };
    is_import_meta_url(arg, source)
}

fn is_method_call_with_import_meta_url(
    call: &CallExpression,
    expected_object: &str,
    expected_method: &str,
    source: &str,
) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name.as_str() != expected_method {
        return false;
    }
    let obj_text = &source[member.object.span().start as usize..member.object.span().end as usize];
    if obj_text != expected_object {
        return false;
    }
    let Some(arg) = single_call_arg(call) else {
        return false;
    };
    // The single arg must be a call to `fileURLToPath(import.meta.url)`
    let Expression::CallExpression(inner_call) = arg else {
        return false;
    };
    is_call_to_with_import_meta_url(inner_call, "fileURLToPath", source)
}

fn is_call_to_with_import_meta_url_two_levels(
    call: &CallExpression,
    outer: &str,
    inner: &str,
    source: &str,
) -> bool {
    let Expression::Identifier(id) = &call.callee else {
        return false;
    };
    if id.name.as_str() != outer {
        return false;
    }
    let Some(arg) = single_call_arg(call) else {
        return false;
    };
    let Expression::CallExpression(inner_call) = arg else {
        return false;
    };
    is_call_to_with_import_meta_url(inner_call, inner, source)
}

fn has_dirname_wrapper_parent(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    // Walk up: node -> argument position -> CallExpression
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let parent = nodes.get_node(parent_id);

    // The parent might be an Argument wrapper, or directly the CallExpression
    let call_node = match parent.kind() {
        AstKind::CallExpression(_) => parent,
        _ => {
            let gp_id = nodes.parent_id(parent_id);
            if gp_id == parent_id {
                return false;
            }
            let gp = nodes.get_node(gp_id);
            match gp.kind() {
                AstKind::CallExpression(_) => gp,
                _ => return false,
            }
        }
    };

    let AstKind::CallExpression(outer_call) = call_node.kind() else {
        return false;
    };

    match &outer_call.callee {
        Expression::Identifier(id) => id.name.as_str() == "dirname",
        Expression::StaticMemberExpression(member) => {
            let obj_text = &source[member.object.span().start as usize..member.object.span().end as usize];
            obj_text == "path" && member.property.name.as_str() == "dirname"
        }
        _ => false,
    }
}

/// Local name of a call's identifier callee (`foo(x)` → `foo`), or `None` when
/// the callee is not a bare identifier.
fn call_callee_name<'a>(call: &'a CallExpression<'a>) -> Option<&'a str> {
    match &call.callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// True when `local_name` is the local binding of any import from one of
/// `modules` (matched on the root package, so `node:url` and a `url/foo`
/// subpath both count). Covers named, default, and namespace specifiers. A
/// `None` name (callee was not an identifier) never resolves to an import.
fn is_imported_from(
    local_name: Option<&str>,
    modules: &[&str],
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(local_name) = local_name else {
        return false;
    };
    semantic.nodes().iter().any(|node| {
        let AstKind::ImportDeclaration(decl) = node.kind() else {
            return false;
        };
        if !modules.contains(&import_root_package(decl.source.value.as_str())) {
            return false;
        }
        let Some(specifiers) = &decl.specifiers else {
            return false;
        };
        specifiers.iter().any(|spec| {
            let local = match spec {
                ImportDeclarationSpecifier::ImportSpecifier(named) => &named.local,
                ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => &def.local,
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => &ns.local,
            };
            local.name.as_str() == local_name
        })
    })
}

/// True when the `path` namespace object of a `path.dirname(...)` call resolves
/// to a default/namespace import from `node:path`/`path`.
fn namespace_is_node_path(call: &CallExpression, semantic: &oxc_semantic::Semantic) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    is_imported_from(Some(obj.name.as_str()), PATH_MODULES, semantic)
}

/// True when the inner `fileURLToPath(import.meta.url)` argument of a wrapper
/// call resolves to a `fileURLToPath` imported from `node:url`/`url`.
fn inner_filename_call_resolves_to_node_url(
    call: &CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(arg) = single_call_arg(call) else {
        return false;
    };
    let Expression::CallExpression(inner) = arg else {
        return false;
    };
    is_imported_from(call_callee_name(inner), URL_MODULES, semantic)
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.mjs")
    }

    #[test]
    fn flags_node_url_filename() {
        let src = r#"import { fileURLToPath } from "node:url";
const x = fileURLToPath(import.meta.url);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bare_url_filename() {
        let src = r#"import { fileURLToPath } from "url";
const x = fileURLToPath(import.meta.url);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_third_party_filename() {
        let src = r#"import { fileURLToPath } from "mlly";
const x = fileURLToPath(import.meta.url);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_third_party_ufo_filename() {
        let src = r#"import { fileURLToPath } from "ufo";
const x = fileURLToPath(import.meta.url);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_unimported_filename() {
        let src = "const x = fileURLToPath(import.meta.url);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_node_dirname_bare() {
        let src = r#"import { fileURLToPath } from "node:url";
import { dirname } from "node:path";
const d = dirname(fileURLToPath(import.meta.url));"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_third_party_dirname() {
        let src = r#"import { fileURLToPath } from "node:url";
import { dirname } from "pathe";
const d = dirname(fileURLToPath(import.meta.url));"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_dirname_with_third_party_fileurltopath() {
        let src = r#"import { fileURLToPath } from "mlly";
import { dirname } from "node:path";
const d = dirname(fileURLToPath(import.meta.url));"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_path_dirname_namespace() {
        let src = r#"import { fileURLToPath } from "node:url";
import path from "node:path";
const d = path.dirname(fileURLToPath(import.meta.url));"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_path_dirname_third_party_namespace() {
        let src = r#"import { fileURLToPath } from "node:url";
import path from "pathe";
const d = path.dirname(fileURLToPath(import.meta.url));"#;
        assert!(run(src).is_empty());
    }
}
