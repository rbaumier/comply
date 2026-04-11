use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let len = lines.len();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if !trimmed.contains("switch") || !trimmed.contains('(') {
                continue;
            }
            // Rough check: line contains `switch` followed by `(`.
            if let Some(sw_pos) = trimmed.find("switch") {
                let after = &trimmed[sw_pos + 6..];
                // Must be `switch (...` or `switch(` — not `switchCase` etc.
                if !after.starts_with(' ') && !after.starts_with('(') {
                    continue;
                }
            } else {
                continue;
            }

            // Count `case ` keywords until the switch block closes.
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

        diagnostics
    }
}

/// Count `case ` keywords within the brace-delimited switch body.
fn count_cases(switch_line: usize, lines: &[&str], len: usize) -> usize {
    let mut depth: i32 = 0;
    let mut found_open = false;
    let mut cases = 0;

    for i in switch_line..len {
        for ch in lines[i].chars() {
            if ch == '{' {
                depth += 1;
                found_open = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }

        // Count `case ` on lines inside the switch body (after opening brace).
        if found_open && i > switch_line {
            let trimmed = lines[i].trim();
            if trimmed.starts_with("case ") || trimmed.starts_with("case\t") {
                cases += 1;
            }
        }
        // Also handle `case` on same line as `switch` — unlikely but possible
        // after the opening brace on the switch line itself.

        if found_open && depth <= 0 {
            break;
        }
    }

    cases
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_switch_with_two_cases() {
        let src = r#"
switch (x) {
  case 1:
    break;
  case 2:
    break;
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-small-switch");
    }

    #[test]
    fn flags_switch_with_one_case() {
        let src = r#"
switch (action.type) {
  case "INCREMENT":
    return state + 1;
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_switch_with_three_cases() {
        let src = r##"
switch (color) {
  case "red":
    return "#f00";
  case "green":
    return "#0f0";
  case "blue":
    return "#00f";
}
"##;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_switch_with_many_cases() {
        let src = r#"
switch (day) {
  case 1: return "Mon";
  case 2: return "Tue";
  case 3: return "Wed";
  case 4: return "Thu";
}
"#;
        assert!(run(src).is_empty());
    }
}
