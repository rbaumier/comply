use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    let func_text = func.utf8_text(source).unwrap_or("");
    if func_text != "t" && func_text != "i18n.t" { return; }

    let args = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };
    let first = match args.named_child(0) {
        Some(a) => a,
        None => return,
    };
    if first.kind() == "template_string" || first.kind() == "binary_expression" {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &first,
            super::META.id,
            "Dynamic `t()` key can't be statically extracted by i18next — use a full static key string.".into(),
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
    fn flags_concat_key() {
        assert_eq!(run("t('section.' + name)").len(), 1);
    }
    #[test]
    fn flags_template_key() {
        assert_eq!(run("t(`nav.${route}`)").len(), 1);
    }
    #[test]
    fn allows_static_key() {
        assert!(run("t('section.home')").is_empty());
    }
}
