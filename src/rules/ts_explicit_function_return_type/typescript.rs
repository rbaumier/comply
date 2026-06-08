//! ts-explicit-function-return-type backend — flag any function
//! (declaration, expression, arrow, method) that lacks an explicit return
//! type annotation.
//!
//! In tree-sitter-typescript, the return type is a `type_annotation` child
//! placed after `parameters`. If no such child exists as a direct child of
//! the function node, the return type is inferred — which we flag.
//!
//! Trivially-typed expressions (e.g. `() => 1`) are NOT exempted here —
//! this rule enforces annotations on every function. If you want a laxer
//! mode, use `ts-explicit-module-boundary-types` which only checks exports.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "function_declaration"
        && kind != "function_expression"
        && kind != "arrow_function"
        && kind != "method_definition"
    {
        return;
    }
    // Only inspect named nodes — the bare `function` keyword is a non-named
    // child of `function_declaration` and must not be walked as a function.
    if !node.is_named() {
        return;
    }

    // Skip setters — setters cannot have a return type.
    if kind == "method_definition" && is_setter(node, source) {
        return;
    }

    // Skip constructors — they don't take a return type.
    if kind == "method_definition" && is_constructor(node, source) {
        return;
    }

    if has_return_type(node) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-explicit-function-return-type".into(),
        message: "Missing return type on function.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

/// True if the function node has a direct `type_annotation` child — the
/// return-type slot in tree-sitter-typescript.
fn has_return_type(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|c| c.kind() == "type_annotation")
}

fn is_setter(node: tree_sitter::Node, source: &[u8]) -> bool {
    method_has_keyword(node, source, "set")
}

fn is_constructor(node: tree_sitter::Node, source: &[u8]) -> bool {
    // Constructor names are `property_identifier` with text "constructor".
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .is_some_and(|t| t == "constructor")
}

fn method_has_keyword(node: tree_sitter::Node, source: &[u8], keyword: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Ok(text) = child.utf8_text(source)
            && text == keyword
        {
            return true;
        }
    }
    false
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
    fn flags_function_without_return_type() {
        assert_eq!(run_on("function foo() { return 1; }").len(), 1);
    }

    #[test]
    fn flags_arrow_without_return_type() {
        assert_eq!(run_on("const f = () => 1;").len(), 1);
    }

    #[test]
    fn allows_function_with_return_type() {
        assert!(run_on("function foo(): number { return 1; }").is_empty());
    }

    #[test]
    fn allows_arrow_with_return_type() {
        assert!(run_on("const f = (): number => 1;").is_empty());
    }

    #[test]
    fn allows_constructor_without_return_type() {
        assert!(run_on("class A { constructor() {} }").is_empty());
    }
}
