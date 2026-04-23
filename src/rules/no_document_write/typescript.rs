//! no-document-write backend — flag `document.write` / `document.writeln` calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if name != "document.write" && name != "document.writeln" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-document-write".into(),
        message: format!("`{name}()` is an XSS vector and re-opens the document — use DOM APIs (`appendChild`, sanitized `innerHTML`) instead."),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_document_write() {
        assert_eq!(run_on(r#"document.write("<p>hi</p>");"#).len(), 1);
    }

    #[test]
    fn flags_document_writeln() {
        assert_eq!(run_on(r#"document.writeln("hi");"#).len(), 1);
    }

    #[test]
    fn allows_other_document_method() {
        assert!(run_on("document.createElement('div');").is_empty());
    }
}
