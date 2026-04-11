use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Invisible Unicode codepoints that should not appear literally in regex.
fn is_invisible_char(c: char) -> bool {
    matches!(c,
        '\u{00AD}'         // soft hyphen
        | '\u{034F}'       // combining grapheme joiner
        | '\u{061C}'       // arabic letter mark
        | '\u{115F}'       // hangul choseong filler
        | '\u{1160}'       // hangul jungseong filler
        | '\u{17B4}'       // khmer vowel inherent aq
        | '\u{17B5}'       // khmer vowel inherent aa
        | '\u{180E}'       // mongolian vowel separator
        | '\u{2000}'..='\u{200F}' // various spaces + zero-width + directional marks
        | '\u{202A}'..='\u{202E}' // bidi embedding / override
        | '\u{2060}'..='\u{2064}' // word joiner, invisible times/separator/plus
        | '\u{2066}'..='\u{206F}' // bidi isolates + deprecated formatting
        | '\u{FE00}'..='\u{FE0F}' // variation selectors
        | '\u{FEFF}'       // BOM / zero-width no-break space
        | '\u{FFF9}'..='\u{FFFB}' // interlinear annotations
    )
}

fn has_invisible_in_regex(line: &str) -> bool {
    if !line.contains('/') && !line.contains("RegExp") && !line.contains("Regex::") {
        return false;
    }
    line.chars().any(is_invisible_char)
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_invisible_in_regex(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-invisible-character".into(),
                    message: "Invisible Unicode character in regex — use an explicit `\\u{...}` escape instead.".into(),
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
    fn flags_zero_width_space() {
        // U+200B zero-width space embedded in regex
        assert_eq!(run("const re = /foo\u{200B}bar/;").len(), 1);
    }

    #[test]
    fn flags_soft_hyphen() {
        assert_eq!(run("const re = /test\u{00AD}word/;").len(), 1);
    }

    #[test]
    fn allows_clean_regex() {
        assert!(run("const re = /foo/;").is_empty());
    }

    #[test]
    fn allows_non_regex_line() {
        assert!(run("const x = 42;").is_empty());
    }
}
