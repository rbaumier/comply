use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["ternary_expression"] => |node, source, ctx, diagnostics|
    let condition = match node.child_by_field_name("condition") {
        Some(c) => c,
        None => return,
    };
    let cond_text = condition.utf8_text(source).unwrap_or("");
    if !cond_text.contains("count") && !cond_text.contains("length") && !cond_text.contains(".size") {
        return;
    }
    if !cond_text.contains("=== 1") && !cond_text.contains("== 1") && !cond_text.contains("> 1") {
        return;
    }
    let consequence = match node.child_by_field_name("consequence") {
        Some(c) => c,
        None => return,
    };
    let alternative = match node.child_by_field_name("alternative") {
        Some(a) => a,
        None => return,
    };
    let cons_text = consequence.utf8_text(source).unwrap_or("");
    let alt_text = alternative.utf8_text(source).unwrap_or("");
    if cons_text.starts_with("t(") && alt_text.starts_with("t(") {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Use `t('key', { count })` for pluralization — manual ternaries break CLDR plural rules.".into(),
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
    use super::Check;
    use crate::diagnostic::Diagnostic;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }
    #[test]
    fn flags_manual_plural() {
        assert_eq!(run("count === 1 ? t('item') : t('items')").len(), 1);
    }
    #[test]
    fn allows_t_with_count() {
        assert!(run("t('item', { count })").is_empty());
    }
    #[test]
    fn allows_non_translation_ternary() {
        assert!(run("count === 1 ? 'item' : 'items'").is_empty());
    }
}
