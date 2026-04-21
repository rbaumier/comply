use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diags = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("v-html") {
                continue;
            }
            if line.contains("sanitize(") || line.contains("DOMPurify") {
                continue;
            }
            let prev_has_sanitize =
                i > 0 && (lines[i - 1].contains("sanitize") || lines[i - 1].contains("// safe"));
            if !prev_has_sanitize {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: line.find("v-html").unwrap_or(0) + 1,
                    rule_id: super::META.id.into(),
                    message: "`v-html` without sanitization is an XSS risk. Wrap the value in `DOMPurify.sanitize(...)`.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src))
    }
    #[test]
    fn flags_v_html_no_sanitize() {
        assert_eq!(run("<div v-html=\"userContent\" />").len(), 1);
    }
    #[test]
    fn allows_v_html_with_sanitize() {
        assert!(run("<div v-html=\"DOMPurify.sanitize(content)\" />").is_empty());
    }
}
