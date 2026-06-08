//! tailwind-no-apply-for-variants — flag `@apply` directives outside
//! `@layer base` / `@layer typography` blocks.
//!
//! The original detection only fires for `.css` files. The rule is
//! registered for TS/JS/TSX/Rust/Vue (none of which is CSS), so this
//! AstCheck never produces diagnostics — it preserves the no-op behaviour
//! the previous TextCheck had on those languages. The actual `.css`
//! enforcement lives elsewhere; until that lands here as a `Language::Css`
//! backend, this `Check` is a placeholder that lets the rule register on
//! the existing language set without panicking.

crate::ast_check! { |_node, _source, _ctx, _diagnostics|
    // No-op on TS/JS/TSX/Rust/Vue — the previous TextCheck filtered out
    // every non-CSS file extension before producing a diagnostic.
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

    #[test]
    fn never_flags_typescript_source() {
        let src = r#"const css = "@apply px-4 py-2 rounded";"#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    #[test]
    fn never_flags_layered_apply_in_ts_string() {
        let src = r#"const css = ".btn { @apply px-4; }";"#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }
}
