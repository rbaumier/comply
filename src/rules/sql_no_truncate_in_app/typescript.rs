//! sql-no-truncate-in-app — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(TS_STRING_KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        if !super::sql_uses_truncate(text) {
            return;
        }
        // Discriminate real SQL `TRUNCATE` statements from Tailwind's
        // `truncate` utility class, which appears in JSX className strings.
        if !super::looks_like_sql_truncate(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`TRUNCATE` bypasses triggers and audit — use `DELETE FROM` instead.".into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_truncate_table() {
        let src = r#"const q = "TRUNCATE TABLE users";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_truncate_bare_with_sql_signal() {
        // `TRUNCATE <ident>` alone is ambiguous with Tailwind's
        // `truncate <class>` strings. We require an extra SQL signal
        // (here, the trailing `;`) to confirm intent.
        let src = r#"const q = "TRUNCATE users;";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_truncate_with_cascade() {
        let src = r#"const q = "TRUNCATE users CASCADE";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_delete_from() {
        let src = r#"const q = "DELETE FROM users";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_tailwind_truncate_class() {
        // `truncate` is a Tailwind CSS utility (overflow + ellipsis),
        // not the SQL keyword. Must not flag.
        let src = r#"const cls = "truncate flex items-center";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_tailwind_truncate_in_jsx_classname() {
        let src = r#"const el = <span className="truncate">hello</span>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_tailwind_arbitrary_variant_truncate() {
        let src = r#"const cls = "[&>span:last-child]:truncate";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_tailwind_truncate_in_template_literal() {
        let src = r#"const cls = `truncate text-sm text-gray-500`;"#;
        assert!(run(src).is_empty());
    }
}
