use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_jsx(ctx: &CheckCtx) -> bool {
    let path = ctx.path.to_string_lossy();
    if path.ends_with(".tsx") || path.ends_with(".jsx") {
        return true;
    }
    let src = ctx.source;
    if src.contains("React") {
        return true;
    }
    src.as_bytes()
        .windows(2)
        .any(|w| w[0] == b'<' && w[1].is_ascii_uppercase())
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("accessKey=") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-no-access-key".into(),
                    message: "Avoid `accessKey` — it conflicts with screen reader keyboard shortcuts.".into(),
                    severity: Severity::Warning,
                });
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
    fn flags_access_key() {
        assert_eq!(run("<button accessKey=\"s\">Save</button>").len(), 1);
    }

    #[test]
    fn flags_access_key_on_div() {
        assert_eq!(run("<div accessKey=\"h\">Help</div>").len(), 1);
    }

    #[test]
    fn allows_elements_without_access_key() {
        assert!(run("<button onClick={save}>Save</button>").is_empty());
    }
}
