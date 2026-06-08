//! ts-no-empty-function backend — flag functions/methods with empty bodies
//! that contain no comments.
//!
//! Detection: walk function-like nodes, check if the statement_block body
//! has no named children and no comments (by byte range heuristic).

use crate::diagnostic::{Diagnostic, Severity};

/// Function-like node kinds whose body we inspect.
const FN_KINDS: &[&str] = &[
    "function_declaration",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function_declaration",
    "generator_function",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if !FN_KINDS.contains(&kind) {
        return;
    }
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    if body.kind() != "statement_block" {
        return;
    }
    // Check if body has any named children (statements)
    let mut cursor = body.walk();
    let has_statements = body.named_children(&mut cursor).any(|c| c.kind() != "comment");
    if has_statements {
        return;
    }
    // Check if there's a comment inside the body (even unnamed)
    let body_text = &source[body.byte_range()];
    if let Ok(text) = std::str::from_utf8(body_text) {
        let inner = text.trim();
        // Strip the outer braces
        if inner.len() > 2 {
            let inner = &inner[1..inner.len() - 1].trim();
            if inner.starts_with("//") || inner.starts_with("/*") {
                return;
            }
        }
    }
    // Skip constructors with parameter properties (TSParameterProperty)
    if kind == "method_definition" {
        // Check if this is a constructor with parameter properties
        let Some(name_node) = node.child_by_field_name("name") else {
            // still report
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "ts-no-empty-function".into(),
                message: "Unexpected empty function.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        };
        let name_text = &source[name_node.byte_range()];
        if name_text == b"constructor" {
            // Check if constructor has accessibility modifiers on params
            if let Some(params) = node.child_by_field_name("parameters") {
                let mut pc = params.walk();
                for param in params.named_children(&mut pc) {
                    // required_parameter with accessibility_modifier or
                    // "public_field_definition" inside params indicates
                    // parameter properties. In tree-sitter-typescript,
                    // parameter properties show as required_parameter with
                    // an accessibility modifier child.
                    if param.kind() == "required_parameter" {
                        let mut cc = param.walk();
                        for child in param.children(&mut cc) {
                            if child.kind() == "accessibility_modifier" {
                                return; // has parameter properties, allow
                            }
                        }
                    }
                }
            }
        }
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-empty-function".into(),
        message: "Unexpected empty function.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
    fn flags_empty_function() {
        let diags = run_on("function foo() {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_arrow_function() {
        let diags = run_on("const foo = () => {};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_function_with_body() {
        assert!(run_on("function foo() { return 1; }").is_empty());
    }

    #[test]
    fn allows_function_with_comment() {
        assert!(run_on("function foo() { /* intentional */ }").is_empty());
    }
}
