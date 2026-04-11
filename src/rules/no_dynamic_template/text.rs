use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const PATTERNS: &[&str] = &[
    ".innerHTML =",
    "document.write(",
    ".insertAdjacentHTML(",
    "v-html=",
    "dangerouslySetInnerHTML",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for pattern in PATTERNS {
                if line.contains(pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-dynamic-template".into(),
                        message: format!(
                            "Dynamic HTML construction via `{}` — use safe DOM APIs or framework escaping instead.",
                            pattern.trim(),
                        ),
                        severity: Severity::Warning,
                    });
                    break; // one diagnostic per line
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_innerhtml() {
        assert_eq!(run("el.innerHTML = '<b>' + name + '</b>';").len(), 1);
    }

    #[test]
    fn flags_document_write() {
        assert_eq!(run("document.write('<script>alert(1)</script>');").len(), 1);
    }

    #[test]
    fn flags_insert_adjacent_html() {
        assert_eq!(run("el.insertAdjacentHTML('beforeend', html);").len(), 1);
    }

    #[test]
    fn flags_v_html() {
        assert_eq!(run("<div v-html=\"rawHtml\"></div>").len(), 1);
    }

    #[test]
    fn allows_text_content() {
        assert!(run("el.textContent = name;").is_empty());
    }
}
