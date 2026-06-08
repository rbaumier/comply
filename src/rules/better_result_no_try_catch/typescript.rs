use crate::diagnostic::{Diagnostic, Severity};

fn imports_better_result(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "better-result") || crate::oxc_helpers::source_contains(source, "@better-result")
}

crate::ast_check! { on ["try_statement"] prefilter = ["better-result"] => |node, _source, ctx, diagnostics|
    if !imports_better_result(ctx.source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Replace try/catch with Result.try({ try, catch }) in better-result modules.".into(),
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
    fn flags_try_catch_in_better_result_module() {
        let src = "import { Result } from 'better-result';\ntry { f(); } catch (e) { g(); }";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_try_catch_when_no_better_result() {
        let src = "try { f(); } catch (e) { g(); }";
        assert!(run(src).is_empty());
    }
}
