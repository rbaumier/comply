use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["Regex::new"] => |node, source, ctx, diagnostics|
    let Some(func_node) = node.child_by_field_name("function") else { return };
    let Ok(func_text) = func_node.utf8_text(source) else { return };
    if func_text != "Regex::new" { return; }

    let Ok(text) = node.utf8_text(source) else { return };

    let pattern_len = extract_string_arg_len(text);
    if pattern_len < super::MIN_PATTERN_LEN { return; }

    let row = node.start_position().row;
    let src_str = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = src_str.lines().collect();

    if row > 0 && lines.get(row - 1).is_some_and(|l| {
        let t = l.trim();
        t.starts_with("//") || t.starts_with("///")
    }) {
        return;
    }
    if lines.get(row).is_some_and(|l| {
        if let Some(rx) = l.find("Regex")
            && let Some(cm) = l.find("//") {
                return cm > rx;
            }
        false
    }) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Complex regex without a comment — add a description of what it matches.".into(),
        Severity::Warning,
    ));
}

fn extract_string_arg_len(call_text: &str) -> usize {
    let after_paren = match call_text.find('(') {
        Some(p) => &call_text[p + 1..],
        None => return 0,
    };

    if let Some(rest) = after_paren.strip_prefix("r\"") {
        return rest.find('"').unwrap_or(0);
    }
    if let Some(rest) = after_paren.strip_prefix("r#\"") {
        return rest.find("\"#").unwrap_or(0);
    }
    if after_paren.starts_with('"') {
        return after_paren[1..].find('"').unwrap_or(0);
    }
    0
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
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_undocumented_complex_regex() {
        let src = "fn f() { let re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_documented_regex() {
        let src = "fn f() {\n// ISO 8601 duration pattern\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_short_regex() {
        let src = "fn f() { let re = Regex::new(r\"^\\d+$\").unwrap(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_inline_comment() {
        let src = "fn f() { let re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap(); // duration\n}";
        assert!(run(src).is_empty());
    }
}
