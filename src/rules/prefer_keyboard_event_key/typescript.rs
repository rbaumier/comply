use crate::diagnostic::{Diagnostic, Severity};

const DEPRECATED_PROPS: &[&str] = &["keyCode", "charCode", "which"];

crate::ast_check! { on ["member_expression"] => |node, source, ctx, diagnostics|
    // Flag `<event>.keyCode` / `.charCode` / `.which` member access. Walking
    // `member_expression` (instead of textual scanning) keeps comments,
    // strings, and unrelated identifiers from triggering false positives.
    let Some(prop) = node.child_by_field_name("property") else {
        return;
    };
    let Ok(prop_text) = std::str::from_utf8(&source[prop.byte_range()]) else {
        return;
    };
    if !DEPRECATED_PROPS.contains(&prop_text) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "prefer-keyboard-event-key",
        format!("Use `.key` instead of `.{prop_text}`."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_event_keycode() {
        assert_eq!(run_ts("if (event.keyCode === 13) {}").len(), 1);
    }

    #[test]
    fn flags_event_which() {
        assert_eq!(run_ts("if (e.which === 27) {}").len(), 1);
    }

    #[test]
    fn flags_event_charcode() {
        assert_eq!(run_ts("const code = event.charCode;").len(), 1);
    }

    #[test]
    fn allows_event_key() {
        assert!(run_ts("if (event.key === 'Enter') {}").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run_ts("// event.keyCode is deprecated").is_empty());
    }
}
