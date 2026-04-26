use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };

    // Check for .postMessage or postMessage
    let is_post_message = match func.kind() {
        "member_expression" => {
            if let Some(prop) = func.child_by_field_name("property") {
                prop.utf8_text(source).unwrap_or("") == "postMessage"
            } else { false }
        }
        "identifier" => func.utf8_text(source).unwrap_or("") == "postMessage",
        _ => false,
    };

    if !is_post_message { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };

    // postMessage(message, targetOrigin, [transfer])
    // Check second argument (targetOrigin)
    let origin_arg = args.named_child(1);

    let is_unsafe = match origin_arg {
        None => true, // Missing targetOrigin
        Some(arg) => {
            let text = arg.utf8_text(source).unwrap_or("");
            // Unsafe if '*' (any origin)
            text == "\"*\"" || text == "'*'" || text == "`*`"
        }
    };

    if !is_unsafe { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "post-message-origin".into(),
        message: "`postMessage()` with `'*'` or missing target origin — specify explicit origin.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }

    #[test]
    fn flags_wildcard_origin() {
        assert_eq!(run("window.postMessage(data, '*')").len(), 1);
    }

    #[test]
    fn flags_missing_origin() {
        assert_eq!(run("iframe.contentWindow.postMessage(data)").len(), 1);
    }

    #[test]
    fn allows_explicit_origin() {
        assert!(run("window.postMessage(data, 'https://example.com')").is_empty());
    }

    #[test]
    fn allows_location_origin() {
        assert!(run("window.postMessage(data, location.origin)").is_empty());
    }
}
