//! no-built-in-override — TS / JS / TSX backend.
//!
//! Walks every symbol whose name matches a built-in global (`Array`,
//! `Object`, `Promise`, …) and flags it. Skips bindings whose
//! declarator has no initializer — `let Array;` is a forward
//! declaration, not an override. Catches the same shapes as
//! no-globals-shadowing (destructured names, params, function
//! declarations, classes) that the previous variable_declarator-only
//! walker missed.

use oxc_ast::AstKind;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{source_type_for_path, with_semantic};
use crate::rules::backend::CheckCtx;

const BUILTINS: &[&str] = &[
    "Array",
    "Object",
    "String",
    "Map",
    "Set",
    "Promise",
    "JSON",
    "Math",
    "undefined",
    "NaN",
    "Infinity",
];

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_type = source_type_for_path(ctx.path);
        with_semantic(ctx.source, source_type, |semantic| {
            let scoping = semantic.scoping();
            let nodes = semantic.nodes();
            let mut diagnostics = Vec::new();

            for symbol_id in scoping.symbol_ids() {
                let name = scoping.symbol_name(symbol_id);
                if !BUILTINS.contains(&name) {
                    continue;
                }
                let decl_id = scoping.symbol_declaration(symbol_id);
                if !has_initializer(nodes, decl_id) {
                    continue;
                }
                let span = scoping.symbol_span(symbol_id);
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-built-in-override".into(),
                    message: format!("Overriding built-in `{name}` — rename this variable."),
                    severity: Severity::Error,
                    span: None,
                });
            }

            diagnostics
        })
    }
}

/// Whether the declaration node overrides the global binding: a
/// `VariableDeclarator` *with* an initializer, OR a `Function` / `Class`
/// / parameter binding (those always introduce a value).
///
/// Import specifiers are excluded: an `import Set from "./set"` binding is
/// module-scoped and cannot reach `globalThis.Set`, so it is not an
/// override regardless of the imported name.
fn has_initializer(nodes: &oxc_semantic::AstNodes, decl_id: oxc_semantic::NodeId) -> bool {
    let kinds = std::iter::once(nodes.kind(decl_id)).chain(nodes.ancestor_kinds(decl_id));
    for kind in kinds {
        match kind {
            AstKind::VariableDeclarator(decl) => return decl.init.is_some(),
            AstKind::Function(_) | AstKind::Class(_) | AstKind::FormalParameter(_) => return true,
            AstKind::ImportSpecifier(_)
            | AstKind::ImportDefaultSpecifier(_)
            | AstKind::ImportNamespaceSpecifier(_) => return false,
            AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
}

fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_const_array_override() {
        let d = run_on("const Array = [];");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array"));
    }

    #[test]
    fn flags_let_object_override() {
        let d = run_on("let Object = {};");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object"));
    }

    #[test]
    fn flags_promise_override() {
        assert_eq!(run_on("const Promise = null;").len(), 1);
    }

    #[test]
    fn flags_undefined_override() {
        let d = run_on("const undefined = 42;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("undefined"));
    }

    #[test]
    fn allows_normal_variables() {
        assert!(run_on("const myArray = [];").is_empty());
    }

    #[test]
    fn allows_usage_not_assignment() {
        assert!(run_on("const x = Array.from([1, 2, 3]);").is_empty());
    }

    #[test]
    fn flags_function_param_array() {
        // `function f(Array) {}` overrides the global within the
        // function — the previous walker missed parameters entirely.
        assert_eq!(run_on("function f(Array: any) { return Array; }").len(), 1);
    }

    #[test]
    fn allows_default_import_named_after_builtin() {
        // huntabyte/shadcn-svelte field/index.ts: component named after a
        // built-in. A module import binding cannot clobber the global.
        let d = run_on(
            "import Set from \"./field-set.svelte\";\nimport Error from \"./field-error.svelte\";\nexport { Set, Error };",
        );
        assert!(d.is_empty(), "default import named after a built-in is not an override: {d:?}");
    }

    #[test]
    fn allows_named_import_aliased_to_builtin() {
        assert!(run_on("import { thing as Map } from \"./m\";\nexport { Map };").is_empty());
    }

    #[test]
    fn allows_namespace_import_named_after_builtin() {
        assert!(run_on("import * as Promise from \"./p\";\nexport { Promise };").is_empty());
    }
}
