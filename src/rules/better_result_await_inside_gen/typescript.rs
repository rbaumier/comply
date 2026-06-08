use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["Result.gen"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.utf8_text(source).unwrap_or("") != "Result.gen" {
        return;
    }
    // Find any await_expression descendants — but not inside nested Result.gen.
    let mut stack: Vec<tree_sitter::Node<'_>> = vec![node];
    while let Some(n) = stack.pop() {
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            if child.kind() == "await_expression" {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &child,
                    super::META.id,
                    "Inside Result.gen, use `yield* Result.await(...)` instead of `await`.".into(),
                    Severity::Warning,
                ));
                continue;
            }
            // Don't recurse into a nested Result.gen call — it has its own scope.
            if child.kind() == "call_expression"
                && let Some(inner_callee) = child.child_by_field_name("function")
                && inner_callee.utf8_text(source).unwrap_or("") == "Result.gen"
                && child.id() != node.id()
            {
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
    fn flags_await_in_gen() {
        let src =
            "const r = Result.gen(async function* () { const v = await fetch('/'); return v; });";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_yield_await_in_gen() {
        let src = "const r = Result.gen(function* () { const v = yield* Result.await(fetch('/')); return v; });";
        assert!(run(src).is_empty());
    }
}
