use crate::diagnostic::{Diagnostic, Severity};

const LOCALE_METHODS: &[&str] = &["toLocaleDateString", "toLocaleTimeString", "toLocaleString"];

crate::ast_check! { on ["call_expression"] prefilter = ["toLocale"] => |node, source, ctx, diagnostics|
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    if func.kind() != "member_expression" { return; }
    let prop = match func.child_by_field_name("property") {
        Some(p) => p,
        None => return,
    };
    let method = prop.utf8_text(source).unwrap_or("");
    if !LOCALE_METHODS.contains(&method) { return; }

    let args = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };
    if args.named_child_count() == 0 {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("Pass an explicit locale to `.{method}()` — without one, formatting depends on the environment locale."),
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
    fn flags_no_locale() {
        assert_eq!(run("date.toLocaleDateString()").len(), 1);
    }
    #[test]
    fn flags_tolocalestring_no_args() {
        assert_eq!(run("n.toLocaleString()").len(), 1);
    }
    #[test]
    fn allows_with_locale() {
        assert!(run("date.toLocaleDateString(i18n.language, { dateStyle: 'short' })").is_empty());
    }
}
