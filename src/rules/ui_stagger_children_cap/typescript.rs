//! Flag `staggerChildren: <number>` pairs where the literal value > 0.05.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["pair"] prefilter = ["staggerChildren"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return };
    let Ok(key_text) = key.utf8_text(source) else { return };
    if key_text != "staggerChildren" { return; }

    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "number" { return; }
    let Ok(value_text) = value.utf8_text(source) else { return };
    let Ok(n) = value_text.parse::<f64>() else { return };
    if n <= 0.05 { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!("`staggerChildren: {n}` is above 0.05s — lists will feel slow; cap at 0.05."),
        severity: Severity::Warning,
        span: None,
    });
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_large_stagger() {
        let src = r#"const v = { staggerChildren: 0.15 };"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_quarter_second() {
        let src = r#"const v = { staggerChildren: 0.25 };"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_small_stagger() {
        let src = r#"const v = { staggerChildren: 0.03 };"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_at_cap() {
        let src = r#"const v = { staggerChildren: 0.05 };"#;
        assert!(run(src).is_empty());
    }
}
