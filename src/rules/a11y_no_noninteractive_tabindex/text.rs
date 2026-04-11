use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const NON_INTERACTIVE: &[&str] = &["<div", "<span", "<p", "<section"];

fn is_jsx_file(ctx: &CheckCtx) -> bool {
    let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "tsx" || ext == "jsx"
}

/// Check if the line has tabIndex that is NOT -1.
fn has_positive_tabindex(line: &str) -> bool {
    // Match tabIndex={N} where N != -1, or tabIndex="N" where N != -1
    if let Some(pos) = line.find("tabIndex=") {
        let rest = &line[pos + 9..]; // skip "tabIndex="
        // tabIndex={-1} or tabIndex="-1" are OK
        if rest.starts_with("{-1}") || rest.starts_with("\"-1\"") {
            return false;
        }
        return true;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx_file(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let lower = line.to_lowercase();
            if !has_positive_tabindex(line) {
                continue;
            }
            for &tag in NON_INTERACTIVE {
                if lower.contains(tag)
                    && let Some(pos) = lower.find(tag)
                {
                    let after = pos + tag.len();
                    if after >= lower.len()
                        || matches!(
                            lower.as_bytes()[after],
                            b' ' | b'>' | b'/' | b'\t' | b'\n'
                        )
                    {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: pos + 1,
                            rule_id: "a11y-no-noninteractive-tabindex".into(),
                            message: format!(
                                "Non-interactive element `{tag}>` should not have `tabIndex`."
                            ),
                            severity: Severity::Warning,
                        });
                        break;
                    }
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
        Check.check(&CheckCtx::for_test(Path::new("component.tsx"), source))
    }

    #[test]
    fn flags_div_with_tabindex_zero() {
        let d = run(r#"<div tabIndex={0}>Focusable div</div>"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_div_with_tabindex_negative_one() {
        assert!(run(r#"<div tabIndex={-1}>Not focusable</div>"#).is_empty());
    }

    #[test]
    fn allows_button_with_tabindex() {
        assert!(run(r#"<button tabIndex={0}>OK</button>"#).is_empty());
    }

    #[test]
    fn flags_span_with_tabindex() {
        let d = run(r#"<span tabIndex={1}>text</span>"#);
        assert_eq!(d.len(), 1);
    }
}
