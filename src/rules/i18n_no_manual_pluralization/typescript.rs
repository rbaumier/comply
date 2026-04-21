use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "ternary_expression" { return; }
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
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
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
