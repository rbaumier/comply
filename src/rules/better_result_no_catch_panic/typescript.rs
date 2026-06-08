use crate::diagnostic::{Diagnostic, Severity};

fn imports_better_result(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "better-result") || crate::oxc_helpers::source_contains(source, "@better-result")
}

crate::ast_check! { on ["catch_clause"] prefilter = ["better-result"] => |node, source, ctx, diagnostics|
    if !imports_better_result(ctx.source) {
        return;
    }
    let text = node.utf8_text(source).unwrap_or("");
    if text.contains("Panic") {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Do not match/re-handle Panic in a catch — let it propagate.".into(),
            Severity::Warning,
        ));
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
    fn flags_catch_panic() {
        let src = "import { Result } from 'better-result';\ntry { f(); } catch (e) { if (e instanceof Panic) {} }";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_catch_without_panic() {
        let src = "import { Result } from 'better-result';\ntry { f(); } catch (e) { log(e); }";
        assert!(run(src).is_empty());
    }
}
