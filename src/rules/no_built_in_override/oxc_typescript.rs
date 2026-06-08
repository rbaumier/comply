//! OXC backend for no-built-in-override.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

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

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
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
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("Overriding built-in `{name}` — rename this variable."),
                severity: Severity::Error,
                span: None,
            });
        }

        diagnostics
    }
}

/// Whether the declaration node is a `VariableDeclarator` *with* an
/// initializer, OR a `Function` / `Class` / parameter binding (those
/// always introduce a value).
fn has_initializer(nodes: &oxc_semantic::AstNodes, decl_id: oxc_semantic::NodeId) -> bool {
    let kinds = std::iter::once(nodes.kind(decl_id)).chain(nodes.ancestor_kinds(decl_id));
    for kind in kinds {
        match kind {
            AstKind::VariableDeclarator(decl) => return decl.init.is_some(),
            AstKind::Function(_)
            | AstKind::Class(_)
            | AstKind::FormalParameter(_)
            | AstKind::ImportSpecifier(_)
            | AstKind::ImportDefaultSpecifier(_)
            | AstKind::ImportNamespaceSpecifier(_) => return true,
            AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
}
