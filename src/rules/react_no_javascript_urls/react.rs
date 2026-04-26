//! react-no-javascript-urls backend — flag JSX `href`/`src`/`action`
//! attributes whose string value starts with `javascript:`.

use crate::diagnostic::{Diagnostic, Severity};

const URL_ATTRS: &[&str] = &["href", "src", "action", "formAction"];

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::jsx::jsx_attribute_name(node, source) else {
        return;
    };
    if !URL_ATTRS.contains(&name) {
        return;
    }
    let Some(value) = crate::rules::jsx::jsx_attribute_string_value(node, source) else {
        return;
    };
    // Strip whitespace — `javascript: alert(1)` with leading spaces still runs.
    let trimmed = value.trim_start();
    if !trimmed.to_ascii_lowercase().starts_with("javascript:") {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-javascript-urls".into(),
        message: format!("`{name}=\"javascript:…\"` is an XSS vector — use an event handler instead."),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_href_javascript_url() {
        let src = r#"const x = <a href="javascript:alert(1)">click</a>;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_href_javascript_case_insensitive() {
        let src = r#"const x = <a href="JavaScript:void(0)">x</a>;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_src_javascript_url() {
        let src = r#"const x = <iframe src="javascript:alert(1)" />;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_regular_href() {
        let src = r#"const x = <a href="/home">home</a>;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_https_href() {
        let src = r#"const x = <a href="https://example.com">x</a>;"#;
        assert!(run_on(src).is_empty());
    }
}
