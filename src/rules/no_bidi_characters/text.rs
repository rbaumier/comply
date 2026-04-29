use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Unicode bidi control characters — trojan source attack vectors.
fn has_bidi_char(line: &str) -> bool {
    line.chars().any(|c| matches!(c,
        '\u{202A}' // LRE — left-to-right embedding
        | '\u{202B}' // RLE — right-to-left embedding
        | '\u{202C}' // PDF — pop directional formatting
        | '\u{202D}' // LRO — left-to-right override
        | '\u{202E}' // RLO — right-to-left override
        | '\u{2066}' // LRI — left-to-right isolate
        | '\u{2067}' // RLI — right-to-left isolate
        | '\u{2068}' // FSI — first strong isolate
        | '\u{2069}' // PDI — pop directional isolate
        | '\u{200F}' // RLM — right-to-left mark
        | '\u{200E}' // LRM — left-to-right mark
    ))
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "\u{202E}", "\u{202D}", "\u{202A}", "\u{202B}", "\u{202C}",
            "\u{2066}", "\u{2067}", "\u{2068}", "\u{2069}",
        ])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_bidi_char(line) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-bidi-characters".into(),
                    message: "Invisible bidi control character detected — potential trojan-source attack.".into(),
                    severity: Severity::Error,
                    span: None,
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_rlo_char() {
        // U+202E right-to-left override
        let source = "const x = \u{202E}abc;";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_lri_char() {
        let source = "const x = \u{2066}abc;";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_clean_code() {
        assert!(run("const x = 42;").is_empty());
    }

    #[test]
    fn flags_in_comments_too() {
        // Bidi chars in comments are also suspicious.
        let source = "// \u{202A}normal comment";
        assert_eq!(run(source).len(), 1);
    }
}
