use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_jsx_file(ctx: &CheckCtx) -> bool {
    let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "tsx" || ext == "jsx"
}

/// Returns the required ARIA props for a given role.
fn required_props(role: &str) -> &'static [&'static str] {
    match role {
        "checkbox" | "radio" => &["aria-checked"],
        "slider" => &["aria-valuenow", "aria-valuemin", "aria-valuemax"],
        "combobox" => &["aria-expanded"],
        "scrollbar" => &["aria-controls", "aria-valuenow"],
        _ => &[],
    }
}

/// Extract the role value from `role="value"`.
fn extract_role(line: &str) -> Option<String> {
    let key = "role=\"";
    if let Some(pos) = line.find(key) {
        let start = pos + key.len();
        let rest = &line[start..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx_file(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if let Some(role) = extract_role(line) {
                let props = required_props(&role);
                if props.is_empty() {
                    continue;
                }
                // Check within a 3-line window
                let window_end = std::cmp::min(idx + 4, lines.len());
                let window = lines[idx..window_end].join(" ");
                let missing: Vec<&str> = props
                    .iter()
                    .filter(|prop| !window.contains(**prop))
                    .copied()
                    .collect();
                if !missing.is_empty() {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "a11y-role-has-required-aria-props".into(),
                        message: format!(
                            "`role=\"{}\"` is missing required ARIA props: {}.",
                            role,
                            missing.join(", ")
                        ),
                        severity: Severity::Error,
                    });
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), source))
    }

    #[test]
    fn flags_checkbox_missing_aria_checked() {
        assert_eq!(run(r#"<div role="checkbox">"#).len(), 1);
    }

    #[test]
    fn allows_checkbox_with_aria_checked() {
        assert!(run(r#"<div role="checkbox" aria-checked="false">"#).is_empty());
    }

    #[test]
    fn flags_slider_missing_props() {
        let diags = run(r#"<div role="slider" aria-valuenow={5}>"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("aria-valuemin"));
    }

    #[test]
    fn allows_slider_with_all_props() {
        let src = r#"<div role="slider"
  aria-valuenow={5}
  aria-valuemin={0}
  aria-valuemax={10}
/>"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_combobox_missing_expanded() {
        assert_eq!(run(r#"<div role="combobox">"#).len(), 1);
    }

    #[test]
    fn skips_non_jsx_files() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("t.ts"), r#"<div role="checkbox">"#));
        assert!(diags.is_empty());
    }
}
