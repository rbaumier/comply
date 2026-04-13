//! explicit-return-type-on-exported backend — exported functions must
//! declare their return type.
//!
//! Why: exported functions form the module's public API. Relying on
//! inference means the return type silently changes when the implementation
//! changes, and every downstream consumer gets recompiled with subtly
//! different types. An explicit annotation is a contract that fails loud
//! when drift happens.
//!
//! Internal (non-exported) functions are fine to leave inferred — the
//! skill rule is "Explicit return types on exports, inference for internals".
//!
//! Detection: walk `export_statement` nodes whose child is a
//! `function_declaration`. Check the function's children for a
//! `type_annotation` sibling to `formal_parameters` — if absent, emit.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let root = tree.root_node();
        let mut diagnostics = Vec::new();

        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if child.kind() != "export_statement" {
                continue;
            }
            let Some(func) = find_function_declaration(child) else {
                continue;
            };
            if has_return_type(func) {
                continue;
            }
            let name = func
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source_bytes).ok())
                .unwrap_or("<anonymous>");
            let pos = func.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "explicit-return-type-on-exported".into(),
                message: format!(
                    "Exported function '{name}' has no return type \
                     annotation — the API contract drifts silently when the \
                     implementation changes. Add an explicit `: ReturnType`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

fn find_function_declaration(export: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = export.walk();
    export
        .children(&mut cursor)
        .find(|c| c.kind() == "function_declaration")
}

/// True if the function_declaration has a direct `type_annotation` child
/// — that's the return type spot (after formal_parameters, before the body).
fn has_return_type(func: tree_sitter::Node) -> bool {
    let mut cursor = func.walk();
    func.children(&mut cursor)
        .any(|c| c.kind() == "type_annotation")
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_exported_function_without_return_type() {
        assert_eq!(run_on("export function foo() { return 1; }").len(), 1);
    }

    #[test]
    fn allows_exported_function_with_return_type() {
        assert!(run_on("export function foo(): number { return 1; }").is_empty());
    }

    #[test]
    fn does_not_flag_non_exported_function() {
        assert!(run_on("function helper() { return 1; }").is_empty());
    }

    #[test]
    fn allows_exported_async_function_with_return_type() {
        assert!(run_on("export async function f(): Promise<number> { return 1; }").is_empty());
    }
}
