use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "postMessage" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // The target origin is the second argument (index 1).
    let Some(origin_arg) = args.named_child(1) else { return };
    if origin_arg.kind() != "string" {
        return;
    }

    let text = origin_arg.utf8_text(source).unwrap_or("");
    if text == "\"*\"" || text == "'*'" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-post-message-star".into(),
            message: "`postMessage` with `\"*\"` target origin — specify an explicit origin.".into(),
            severity: Severity::Error,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_double_quote_star() {
        assert_eq!(run(r#"window.postMessage(data, "*");"#).len(), 1);
    }

    #[test]
    fn flags_single_quote_star() {
        assert_eq!(run("iframe.contentWindow.postMessage(msg, '*');").len(), 1);
    }

    #[test]
    fn allows_specific_origin() {
        assert!(run(r#"window.postMessage(data, "https://example.com");"#).is_empty());
    }

    #[test]
    fn allows_variable_origin() {
        assert!(run("window.postMessage(data, targetOrigin);").is_empty());
    }
}
