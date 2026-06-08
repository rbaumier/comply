//! sql-no-truncate-in-app — oxc backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
        if !super::sql_uses_truncate(&text) {
            return;
        }
        if !super::looks_like_sql_truncate(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`TRUNCATE` bypasses triggers and audit — use `DELETE FROM` instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_truncate_table() {
        let src = r#"const q = "TRUNCATE TABLE users";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_delete_from() {
        let src = r#"const q = "DELETE FROM users";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_tailwind_truncate_class() {
        let src = r#"const cls = "truncate flex items-center";"#;
        assert!(run_on(src).is_empty());
    }



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
