use crate::diagnostic::{Diagnostic, Severity};

const DEPRECATED_PROPS: &[&str] = &["keyCode", "charCode", "which"];

/// Detects access to `event.keyCode`, `event.charCode`, or `event.which`.
fn find_deprecated_key_prop(line: &str) -> Option<&'static str> {
    for &prop in DEPRECATED_PROPS {
        let mut start = 0;
        while let Some(pos) = line[start..].find(prop) {
            let abs = start + pos;
            let after = abs + prop.len();
            if abs > 0 {
                let prev = line.as_bytes()[abs - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    start = after;
                    continue;
                }
            }
            if after < line.len() {
                let next = line.as_bytes()[after];
                if next.is_ascii_alphanumeric() || next == b'_' {
                    start = after;
                    continue;
                }
            }
            if abs > 0 {
                let prev = line.as_bytes()[abs - 1];
                if prev == b'.' || prev == b'{' || prev == b',' || prev == b' ' {
                    return Some(prop);
                }
            }
            start = after;
        }
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }
    let src = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in src.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('*') {
            continue;
        }
        if let Some(prop) = find_deprecated_key_prop(trimmed) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "prefer-keyboard-event-key".into(),
                message: format!("Use `.key` instead of `.{prop}`."),
                severity: Severity::Warning,
            });
        }
    }
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
