use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Checks if a regex literal has `\p{...}` or `\P{...}` without the `u` or `v` flag.
fn find_unicode_escape_without_flag(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();

    // Find regex literals: /pattern/flags
    let mut i = 0;
    while i < len {
        // Heuristic: find `/` that starts a regex (after `=`, `(`, `,`, `|`, `!`, `:`, `return`)
        if bytes[i] == b'/' && is_regex_start(line, i) {
            let regex_start = i;
            i += 1;
            let mut has_unicode_prop = false;
            // Scan until closing `/`
            while i < len {
                if bytes[i] == b'\\' && i + 1 < len {
                    if (bytes[i + 1] == b'p' || bytes[i + 1] == b'P') && i + 2 < len && bytes[i + 2] == b'{' {
                        has_unicode_prop = true;
                    }
                    i += 2;
                    continue;
                }
                if bytes[i] == b'/' {
                    break;
                }
                i += 1;
            }
            if i < len && bytes[i] == b'/' && has_unicode_prop {
                // Read flags
                let flag_start = i + 1;
                let mut fi = flag_start;
                while fi < len && bytes[fi].is_ascii_alphabetic() {
                    fi += 1;
                }
                let flags = &line[flag_start..fi];
                if !flags.contains('u') && !flags.contains('v') {
                    hits.push(regex_start);
                }
                i = fi;
            }
            continue;
        }

        // Also check `new RegExp("...", "flags")` patterns
        if i + 11 <= len && bytes[i..].starts_with(b"new RegExp(") {
            let call_start = i;
            let rest = std::str::from_utf8(&bytes[i + 11..]).unwrap_or("");
            let has_prop = rest.contains("\\p{") || rest.contains("\\P{");
            if has_prop {
                // Find flags argument
                if let Some(flags) = extract_regexp_flags(rest) {
                    if !flags.contains('u') && !flags.contains('v') {
                        hits.push(call_start);
                    }
                } else {
                    // No flags argument at all
                    hits.push(call_start);
                }
            }
        }
        i += 1;
    }
    hits
}

fn is_regex_start(line: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let before = line[..pos].trim_end();
    if before.is_empty() {
        return true;
    }
    let last = before.as_bytes()[before.len() - 1];
    matches!(last, b'=' | b'(' | b',' | b'|' | b'!' | b':' | b';' | b'{' | b'[' | b'&')
}

fn extract_regexp_flags(s: &str) -> Option<&str> {
    // Find comma after first argument, then extract second string argument
    let mut depth = 0;
    let mut in_string = None;
    for (i, ch) in s.char_indices() {
        match ch {
            '"' | '\'' | '`' if in_string.is_none() => in_string = Some(ch),
            c if in_string == Some(c) => in_string = None,
            '(' if in_string.is_none() => depth += 1,
            ')' if in_string.is_none() && depth > 0 => depth -= 1,
            ')' if in_string.is_none() => return None,
            ',' if in_string.is_none() && depth == 0 => {
                let rest = s[i + 1..].trim();
                // Extract string content
                if rest.starts_with('"') || rest.starts_with('\'') {
                    let quote = rest.as_bytes()[0];
                    if let Some(end) = rest[1..].find(quote as char) {
                        return Some(&rest[1..1 + end]);
                    }
                }
                return None;
            }
            _ => {}
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_unicode_escape_without_flag(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-use-unicode-flag".into(),
                    message: "Unicode property escape (`\\p{...}`) requires the `u` or `v` flag.".into(),
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
    fn flags_unicode_prop_without_u() {
        let diags = run(r#"const re = /\p{Letter}/;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_uppercase_p_without_u() {
        let diags = run(r#"const re = /\P{Number}/i;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_unicode_prop_with_u() {
        assert!(run(r#"const re = /\p{Letter}/u;"#).is_empty());
    }

    #[test]
    fn allows_unicode_prop_with_v() {
        assert!(run(r#"const re = /\p{Letter}/v;"#).is_empty());
    }
}
