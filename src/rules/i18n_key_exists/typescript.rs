use crate::diagnostic::{Diagnostic, Severity};

use super::is_malformed;

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    let func_text = func.utf8_text(source).unwrap_or("");
    if func_text != "t" && func_text != "i18n.t" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first) = args.named_child(0) else { return };
    if first.kind() != "string" { return; }
    let Ok(raw) = first.utf8_text(source) else { return };
    let inner = raw
        .strip_prefix('"').and_then(|s| s.strip_suffix('"'))
        .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(raw);
    if !is_malformed(inner) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &first,
        super::META.id,
        "t() key is malformed (consecutive/leading/trailing dots, empty segment, or non-alphanumeric character) — it cannot resolve to a locale entry.".into(),
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
    fn flags_double_dot() {
        assert_eq!(run("t('auth..title')").len(), 1);
    }

    #[test]
    fn flags_trailing_dot() {
        assert_eq!(run("t('auth.title.')").len(), 1);
    }

    #[test]
    fn allows_normal_key() {
        assert!(run("t('auth.title')").is_empty());
    }

    #[test]
    fn flags_leading_dot() {
        assert_eq!(run("t('.auth.title')").len(), 1);
    }

    #[test]
    fn flags_slash_in_key() {
        assert_eq!(run("t('auth/title')").len(), 1);
    }

    #[test]
    fn flags_special_char_in_key() {
        assert_eq!(run("t('auth.title!')").len(), 1);
    }
}
