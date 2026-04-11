use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const STATIC_ELEMENTS: &[&str] = &["<div", "<span"];

fn is_jsx_file(ctx: &CheckCtx) -> bool {
    let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "tsx" || ext == "jsx"
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx_file(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let lower = line.to_lowercase();
            if !line.contains("onClick=") {
                continue;
            }
            if lower.contains("role=") {
                continue;
            }
            for &tag in STATIC_ELEMENTS {
                if lower.contains(tag) {
                    if let Some(pos) = lower.find(tag) {
                        let after = pos + tag.len();
                        if after >= lower.len()
                            || matches!(
                                lower.as_bytes()[after],
                                b' ' | b'>' | b'/' | b'\t' | b'\n'
                            )
                        {
                            let element = &tag[1..]; // strip '<'
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: idx + 1,
                                column: pos + 1,
                                rule_id: "a11y-no-static-element-interactions".into(),
                                message: format!(
                                    "Static element `<{element}>` has `onClick` without a `role` attribute."
                                ),
                                severity: Severity::Warning,
                            });
                            break;
                        }
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
    fn flags_div_onclick_without_role() {
        let d = run(r#"<div onClick={handler}>Click</div>"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("div"));
    }

    #[test]
    fn flags_span_onclick_without_role() {
        let d = run(r#"<span onClick={handler}>Click</span>"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("span"));
    }

    #[test]
    fn allows_div_with_role() {
        assert!(run(r#"<div role="button" onClick={handler}>Click</div>"#).is_empty());
    }

    #[test]
    fn allows_button() {
        assert!(run(r#"<button onClick={handler}>Click</button>"#).is_empty());
    }
}
