use crate::diagnostic::{Diagnostic, Severity};

fn imports_better_result(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "better-result") || crate::oxc_helpers::source_contains(source, "@better-result")
}

crate::ast_check! { prefilter = ["better-result"] => |node, source, ctx, diagnostics|
    if !imports_better_result(ctx.source) {
        return;
    }
    // Check function_declaration / method_definition / arrow_function return type annotation
    if !matches!(
        node.kind(),
        "function_declaration" | "method_definition" | "arrow_function" | "function_expression"
    ) {
        return;
    }
    let Some(ret) = node.child_by_field_name("return_type") else { return; };
    let text = ret.utf8_text(source).unwrap_or("");
    // Match patterns like ": T | null" or ": T | undefined"
    let has_nullable = (text.contains("| null") || text.contains("|null"))
        || (text.contains("| undefined") || text.contains("|undefined"));
    if !has_nullable {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &ret,
        super::META.id,
        "Replace nullable return type with Result<T, NotFoundError> in better-result modules.".into(),
        Severity::Warning,
    ));
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
    fn flags_nullable_return() {
        let src =
            "import { Result } from 'better-result';\nfunction f(): User | null { return null; }";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_result_return() {
        let src = "import { Result } from 'better-result';\nfunction f(): Result<User, NotFoundError> { return Result.err(new NotFoundError()); }";
        assert!(run(src).is_empty());
    }
}
