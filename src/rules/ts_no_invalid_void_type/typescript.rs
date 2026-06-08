//! ts-no-invalid-void-type backend — flag `void` used outside of return
//! type annotations and generic type arguments.
//!
//! Detection: walk nodes with kind `void` (tree-sitter type keyword).
//! Allow when parent is a return type annotation of a function or a
//! generic type argument. Flag everywhere else.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["predefined_type"] => |node, source, ctx, diagnostics|
    // tree-sitter-typescript uses "predefined_type" for `void`, `never`, etc.
    let text = &source[node.byte_range()];
    if text != b"void" {
        return;
    }
    let Some(parent) = node.parent() else {
        return;
    };
    // Allow: return type annotation of a function
    // In tree-sitter: type_annotation whose parent is a function-like node
    if parent.kind() == "type_annotation"
        && let Some(grandparent) = parent.parent() {
            let gpk = grandparent.kind();
            if gpk == "function_declaration"
                || gpk == "function"
                || gpk == "arrow_function"
                || gpk == "method_definition"
                || gpk == "function_signature"
                || gpk == "method_signature"
                || gpk == "abstract_method_definition"
                || gpk == "call_signature"
                || gpk == "construct_signature"
                || gpk == "generator_function_declaration"
                || gpk == "generator_function"
            {
                return; // valid return type
            }
        }
    // Allow: generic type argument (type_arguments)
    if parent.kind() == "type_arguments" {
        return;
    }
    // Allow: inside a union type where the parent's parent is a valid
    // return type context
    if parent.kind() == "union_type"
        && let Some(grandparent) = parent.parent()
            && grandparent.kind() == "type_annotation"
                && let Some(ggp) = grandparent.parent() {
                    let gpk = ggp.kind();
                    if gpk == "function_declaration"
                        || gpk == "function"
                        || gpk == "arrow_function"
                        || gpk == "method_definition"
                        || gpk == "function_signature"
                        || gpk == "method_signature"
                    {
                        return; // void in a union return type
                    }
                }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-invalid-void-type".into(),
        message: "`void` is only valid as a return type or generic type argument.".into(),
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
    fn flags_void_variable() {
        let diags = run_on("let x: void;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_void_parameter() {
        let diags = run_on("function foo(x: void) {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_void_return_type() {
        assert!(run_on("function foo(): void {}").is_empty());
    }

    #[test]
    fn allows_void_in_generic() {
        assert!(run_on("let x: Promise<void>;").is_empty());
    }
}
