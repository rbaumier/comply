//! tailwind-no-apply-for-variants — oxc backend (no-op on TS/JS/TSX).

use crate::rules::backend::OxcCheck;

pub struct Check;

impl OxcCheck for Check {
    // No-op on TS/JS/TSX — the actual enforcement targets CSS files.
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn never_flags_typescript_source() {
        assert!(run(r#"const css = "@apply px-4 py-2 rounded";"#).is_empty());
    }

    #[test]
    fn never_flags_layered_apply_in_ts_string() {
        assert!(run(r#"const css = ".btn { @apply px-4; }";"#).is_empty());
    }
}
