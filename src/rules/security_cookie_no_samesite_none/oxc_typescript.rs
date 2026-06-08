use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn find_samesite_none_object(line: &str) -> Option<usize> {
    let lower = line.to_ascii_lowercase();
    if !lower.contains("samesite") {
        return None;
    }
    let mut from = 0;
    while let Some(rel) = lower[from..].find("samesite") {
        let abs = from + rel;
        let after = abs + "samesite".len();
        let mut j = after;
        let bytes = lower.as_bytes();
        while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b':' {
            j += 1;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b'\'' || bytes[j] == b'"') {
                let quote = bytes[j];
                let value_start = j + 1;
                let value_end = lower[value_start..]
                    .find(quote as char)
                    .map(|p| value_start + p);
                if let Some(end) = value_end {
                    let value = &lower[value_start..end];
                    if value == "none" {
                        return Some(abs);
                    }
                }
            }
        }
        from = abs + "samesite".len();
    }
    None
}

fn find_samesite_none_header(line: &str) -> Option<usize> {
    let lower = line.to_ascii_lowercase();
    lower.find("samesite=none")
}

fn has_secure_true_nearby(source: &str, line_idx: usize) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let start = line_idx.saturating_sub(8);
    let end = (line_idx + 8).min(lines.len());
    for line in &lines[start..end] {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("secure") {
            continue;
        }
        if let Some(rel) = lower.find("secure") {
            let after = rel + "secure".len();
            let bytes = lower.as_bytes();
            let mut j = after;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b':' {
                let rest = lower[j + 1..].trim_start();
                if rest.starts_with("false") || rest.starts_with('0') {
                    continue;
                }
                return true;
            }
        }
    }
    false
}

fn header_has_secure_token(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("secure")
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(col) = find_samesite_none_object(line) {
                if !has_secure_true_nearby(ctx.source, idx) {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: super::META.id.into(),
                        message: "`sameSite: 'none'` without `secure: true` — browsers reject \
                                  the cookie and the value is cleartext-leakable. Pair the two."
                            .to_string(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
                continue;
            }
            if let Some(col) = find_samesite_none_header(line) {
                if !header_has_secure_token(line) {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: super::META.id.into(),
                        message: "`SameSite=None` raw cookie header is missing `Secure` — \
                                  browsers reject the cookie. Append `; Secure`."
                            .to_string(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_object_samesite_none_without_secure() {
        let src = "res.cookie('sid', v, { sameSite: 'none', httpOnly: true });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_object_samesite_none_with_secure() {
        let src = "res.cookie('sid', v, { sameSite: 'none', secure: true, httpOnly: true });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_object_samesite_none_with_secure_on_other_line() {
        let src = "res.cookie('sid', v, {\n  sameSite: 'none',\n  secure: true,\n});";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_object_samesite_none_with_secure_false() {
        let src = "res.cookie('sid', v, { sameSite: 'none', secure: false });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_raw_header_samesite_none_without_secure() {
        let src = "res.setHeader('Set-Cookie', 'sid=abc; SameSite=None; HttpOnly');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_raw_header_samesite_none_with_secure() {
        let src = "res.setHeader('Set-Cookie', 'sid=abc; SameSite=None; Secure; HttpOnly');";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_samesite_strict() {
        let src = "res.cookie('sid', v, { sameSite: 'strict' });";
        assert!(run(src).is_empty());
    }
}
