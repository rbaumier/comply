//! tanstack-query-no-keep-previous-data-prop backend.
//!
//! Flag `keepPreviousData: true` pairs. v5 replaced this with
//! `placeholderData: keepPreviousData` (the imported helper).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["pair"] prefilter = ["keepPreviousData"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return; };
    let Ok(key_text) = key.utf8_text(source) else { return; };
    let key_name = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key_name != "keepPreviousData" { return; }
    let Some(value) = node.child_by_field_name("value") else { return; };
    if value.kind() != "true" { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`keepPreviousData: true` was removed in v5 — use `placeholderData: keepPreviousData` instead.".into(),
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
    fn flags() {
        assert_eq!(
            run("useQuery({ queryKey: ['x'], queryFn: f, keepPreviousData: true })").len(),
            1
        );
    }

    #[test]
    fn allows() {
        assert!(
            run("useQuery({ queryKey: ['x'], queryFn: f, placeholderData: keepPreviousData })")
                .is_empty()
        );
    }
}
