//! no-small-switch backend — switch with fewer than 3 cases.

use crate::diagnostic::{Diagnostic, Severity};

/// Count `case ` keywords within the brace-delimited switch body.
fn count_cases(switch_line: usize, lines: &[&str], len: usize) -> usize {
    let mut depth: i32 = 0;
    let mut found_open = false;
    let mut cases = 0;

    for (i, line) in lines.iter().enumerate().take(len).skip(switch_line) {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                found_open = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }

        // Count `case ` on lines inside the switch body (after opening brace).
        if found_open && i > switch_line {
            let trimmed = line.trim();
            if trimmed.starts_with("case ") || trimmed.starts_with("case\t") {
                cases += 1;
            }
        }

        if found_open && depth <= 0 {
            break;
        }
    }

    cases
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = text.lines().collect();
    let len = lines.len();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if !trimmed.contains("switch") || !trimmed.contains('(') {
            continue;
        }
        if let Some(sw_pos) = trimmed.find("switch") {
            let after = &trimmed[sw_pos + 6..];
            if !after.starts_with(' ') && !after.starts_with('(') {
                continue;
            }
        } else {
            continue;
        }

        let case_count = count_cases(idx, &lines, len);
        if case_count < 3 {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-small-switch".into(),
                message: format!(
                    "`switch` has only {} case(s) — use `if/else` instead.",
                    case_count
                ),
                severity: Severity::Warning,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_switch_with_two_cases() {
        let src = "switch (x) {\n  case 1:\n    break;\n  case 2:\n    break;\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-small-switch");
    }

    #[test]
    fn flags_switch_with_one_case() {
        let src = "switch (action.type) {\n  case \"INCREMENT\":\n    return state + 1;\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_switch_with_three_cases() {
        let src = "switch (color) {\n  case \"red\":\n    return \"#f00\";\n  case \"green\":\n    return \"#0f0\";\n  case \"blue\":\n    return \"#00f\";\n}";
        assert!(run_on(src).is_empty());
    }
}
