use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects backreferences that may be useless because some paths to the
/// backreference do not pass through the referenced capturing group.
/// Pattern: `(a)|\1` — the backreference `\1` is in a different alternative
/// than the group it references.
fn find_potentially_useless_backrefs(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();

    // Quick check: must have both a capturing group and a backreference
    if !bytes.contains(&b'\\') {
        return hits;
    }

    let mut i = 0;
    while i < len {
        if bytes[i] == b'/' {
            if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b')') {
                i += 1;
                continue;
            }
            let start = i + 1;
            let mut j = start;
            while j < len {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'/' {
                    let pattern = &line[start..j];
                    if has_cross_alt_backref(pattern) {
                        hits.push(i);
                    }
                    i = j;
                    break;
                }
                j += 1;
            }
        }
        i += 1;
    }
    hits
}

/// Check if a backreference exists in a different top-level alternative
/// than its referenced group.
fn has_cross_alt_backref(pattern: &str) -> bool {
    let alts = split_top_level(pattern);
    if alts.len() < 2 {
        return false;
    }
    for (i, alt) in alts.iter().enumerate() {
        // Find backreferences in this alternative
        let bytes = alt.as_bytes();
        let mut k = 0;
        while k + 1 < bytes.len() {
            if bytes[k] == b'\\' && bytes[k + 1].is_ascii_digit() && bytes[k + 1] != b'0' {
                let group_num = (bytes[k + 1] - b'0') as usize;
                // Check if this group_num's capturing group is in a different alt
                let mut group_count = 0;
                let mut found_in_other = false;
                for (j, other_alt) in alts.iter().enumerate() {
                    for &b in other_alt.as_bytes() {
                        if b == b'(' {
                            group_count += 1;
                            if group_count == group_num && j != i {
                                found_in_other = true;
                            }
                        }
                    }
                }
                if found_in_other {
                    return true;
                }
            }
            k += 1;
        }
    }
    false
}

fn split_top_level(pattern: &str) -> Vec<&str> {
    let mut alts = Vec::new();
    let bytes = pattern.as_bytes();
    let mut depth = 0;
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\\' => {}
            b'(' | b'[' => depth += 1,
            b')' | b']' => { if depth > 0 { depth -= 1; } }
            b'|' if depth == 0 => {
                alts.push(&pattern[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    alts.push(&pattern[start..]);
    alts
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_potentially_useless_backrefs(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-potentially-useless-backreference".into(),
                    message: "Backreference may be useless \u{2014} some paths do not go through the referenced group.".into(),
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
    fn flags_cross_alt_backref() {
        assert_eq!(run(r#"const re = /(a)|\1/;"#).len(), 1);
    }

    #[test]
    fn allows_same_alt_backref() {
        assert!(run(r#"const re = /(a)\1/;"#).is_empty());
    }
}
