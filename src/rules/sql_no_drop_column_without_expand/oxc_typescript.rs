//! sql-no-drop-column-without-expand — oxc backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::sql_helpers::is_sql_ddl;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (text, offset) = match node.kind() {
            AstKind::StringLiteral(lit) => (lit.value.as_str().to_string(), lit.span.start as usize),
            AstKind::TemplateLiteral(tpl) => {
                let s: String = tpl.quasis.iter().map(|q| q.value.raw.as_str()).collect::<Vec<_>>().join(" ");
                (s, tpl.span.start as usize)
            }
            _ => return,
        };
        // Cheap per-node SQL filter before the O(source) file scan: a file of
        // N non-SQL string literals would otherwise cost O(N²).
        if !is_sql_ddl(&text) {
            return;
        }
        if super::file_marks_deprecation(ctx.source) {
            return;
        }
        if !super::sql_drops_column(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "DROP COLUMN without a prior deprecation release breaks running deploys — deprecate first, drop later.".into(),
            severity: Severity::Warning,
            span: None,
        });
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_bare_drop_column() {
        let src = r#"const m = "ALTER TABLE account DROP COLUMN legacy_flag;";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_with_deprecation_marker() {
        let src = "// legacy_flag deprecated in v4.2\nconst m = \"ALTER TABLE account DROP COLUMN legacy_flag;\";";
        assert!(run_on(src).is_empty());
    }

    // Regression for #892: `file_marks_deprecation` scans the whole source, so
    // calling it for every string literal made this rule O(n²). Running the
    // cheap `is_sql_ddl` filter first keeps a file of N non-SQL literals linear.
    // Generous wall-clock budget: post-fix this is tens of ms even in debug.
    #[test]
    fn many_non_sql_literals_stay_linear() {
        let mut src = String::from("const names = [\n");
        for i in 0..20_000 {
            src.push_str("  \"surname");
            src.push_str(&i.to_string());
            src.push_str("\",\n");
        }
        src.push_str("];\n");
        let start = std::time::Instant::now();
        assert!(run_on(&src).is_empty());
        assert!(
            start.elapsed().as_secs() < 5,
            "sql-no-drop-column-without-expand went quadratic: {:?}",
            start.elapsed()
        );
    }
}
