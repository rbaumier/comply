use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const ZWJ: char = '\u{200D}';

/// Check if a `[...]` character class contains chars > U+FFFF or ZWJ sequences.
fn has_misleading_char_class(line: &str) -> bool {
    let mut in_class = false;
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '[' && !in_class {
            in_class = true;
            continue;
        }
        if ch == ']' && in_class {
            in_class = false;
            continue;
        }
        if in_class {
            // Flag chars above BMP or ZWJ
            if ch as u32 > 0xFFFF || ch == ZWJ {
                return true;
            }
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_misleading_char_class(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-misleading-char-class".into(),
                    message: "Character class contains multi-codepoint graphemes — they will be split into individual code points.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_emoji_in_char_class() {
        // U+1F468 is above U+FFFF
        let code = "const re = /[\u{1F468}]/;";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_zwj_in_char_class() {
        // Family emoji with ZWJ
        let code = "const re = /[\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}]/;";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_ascii_char_class() {
        assert!(run("const re = /[abc]/;").is_empty());
    }

    #[test]
    fn allows_emoji_outside_char_class() {
        assert!(run("const re = /\u{1F468}/;").is_empty());
    }
}
