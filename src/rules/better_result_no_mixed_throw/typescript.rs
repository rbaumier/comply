use crate::diagnostic::{Diagnostic, Severity};

fn is_inside_result_try(mut node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    while let Some(parent) = node.parent() {
        if parent.kind() == "call_expression"
            && let Some(callee) = parent.child_by_field_name("function")
        {
            let name = callee.utf8_text(source).unwrap_or("");
            if name == "Result.try" || name == "Result.tryPromise" {
                return true;
            }
        }
        node = parent;
    }
    false
}

crate::ast_check! { on ["function_declaration", "method_definition", "arrow_function", "function_expression"] prefilter = ["Result.try", "Result.tryPromise"] => |node, source, ctx, diagnostics|
    let Some(ret) = node.child_by_field_name("return_type") else { return; };
    let ret_text = ret.utf8_text(source).unwrap_or("");
    if !ret_text.contains("Result<") && !ret_text.contains("Result <") {
        return;
    }
    let Some(body) = node.child_by_field_name("body") else { return; };
    // Find any throw_statement not inside Result.try
    let mut stack: Vec<tree_sitter::Node<'_>> = vec![body];
    while let Some(n) = stack.pop() {
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            if child.kind() == "throw_statement" && !is_inside_result_try(child, source) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &child,
                    super::META.id,
                    "Function returns Result<...> but contains `throw` — return Result.err(...) instead.".into(),
                    Severity::Warning,
                ));
            }
            // Don't descend into nested functions (they have their own return type).
            if matches!(
                child.kind(),
                "function_declaration" | "method_definition" | "arrow_function" | "function_expression"
            ) {
                continue;
            }
            stack.push(child);
        }
    }
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
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }
    #[test]
    fn flags_throw_in_result_function() {
        let src = "function f(): Result<number, E> { throw new Error('x'); }";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_throw_inside_result_try() {
        let src = "function f(): Result<number, E> { return Result.try({ try: () => { throw new Error('x'); }, catch: (e) => new E() }); }";
        assert!(run(src).is_empty());
    }
}
