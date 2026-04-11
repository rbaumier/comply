//! ts-prefer-namespace-keyword backend — flag `module X {}` declarations
//! where the name is an identifier (not a string literal).
//!
//! tree-sitter-typescript parses both `module Foo {}` and `namespace Foo {}`
//! as a `module` (or `internal_module`) node. We check the source text to
//! determine which keyword was used.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let nk = node.kind();
    if nk != "module" && nk != "internal_module" {
        return;
    }

    // Check children for a string name — `declare module "foo" {}` is fine.
    let mut has_string_name = false;
    let mut has_ident_name = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "string" { has_string_name = true; }
        if child.kind() == "identifier" { has_ident_name = true; }
    }
    // Also check via field name (some tree-sitter versions).
    if let Some(name_node) = node.child_by_field_name("name") {
        if name_node.kind() == "string" { has_string_name = true; }
        if name_node.kind() == "identifier" { has_ident_name = true; }
    }

    if has_string_name || !has_ident_name {
        return;
    }

    // Check the source text of the node to detect the keyword.
    let text = match std::str::from_utf8(&source[node.byte_range()]) {
        Ok(t) => t,
        Err(_) => return,
    };

    if text.starts_with("module") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-prefer-namespace-keyword".into(),
            message: "Use `namespace` instead of `module` to declare \
                      custom TypeScript modules."
                .into(),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_module_keyword() {
        let diags = run_on("module Foo { export const x = 1; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_namespace_keyword() {
        assert!(run_on("namespace Foo { export const x = 1; }").is_empty());
    }

    #[test]
    fn allows_string_module() {
        // `declare module "foo" {}` is fine — it's an ambient module.
        assert!(run_on("declare module \"foo\" {}").is_empty());
    }
}
