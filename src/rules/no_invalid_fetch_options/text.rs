use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Scan for `fetch(` or `new Request(` calls whose options object contains
/// `body:` together with `method: 'GET'` / `method: 'HEAD'` (or no method at
/// all, which defaults to GET).
///
/// Strategy: collect consecutive lines that belong to the same call-site
/// (from the `fetch(`/`new Request(` line until paren depth returns to 0),
/// then inspect the joined text for the `body` + `method` combination.
fn check_source(ctx: &CheckCtx) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = ctx.source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let is_fetch = line.contains("fetch(") && !line.contains("fetchOptions") && !line.contains("fetcher");
        let is_request = line.contains("new Request(");

        if !is_fetch && !is_request {
            i += 1;
            continue;
        }

        // Collect all lines of this call expression by tracking paren depth.
        let start_line = i;
        let trigger = if is_fetch { "fetch(" } else { "new Request(" };
        let trigger_pos = line.find(trigger).unwrap();
        let mut depth: i32 = 0;
        let mut block = String::new();

        // Count parens starting from the trigger
        for ch in line[trigger_pos..].chars() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
        }
        block.push_str(&line[trigger_pos..]);

        let mut j = i + 1;
        while depth > 0 && j < lines.len() {
            let next = lines[j];
            block.push(' ');
            block.push_str(next.trim());
            for ch in next.chars() {
                match ch {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
            }
            j += 1;
        }

        // Now analyze the collected block for body + method
        if let Some(diag) = analyze_call_block(&block, ctx, start_line) {
            diagnostics.push(diag);
        }

        i = j.max(i + 1);
    }

    diagnostics
}

/// Given the text of a `fetch(…)` or `new Request(…)` call, check whether
/// it has `body:` with a GET/HEAD method (or default GET).
fn analyze_call_block(
    block: &str,
    ctx: &CheckCtx,
    line_idx: usize,
) -> Option<Diagnostic> {
    // Must have a body property (not `body: undefined` or `body: null`)
    let has_body = has_property(block, "body");
    if !has_body {
        return None;
    }

    // Check if body value is undefined or null
    if let Some(body_val) = get_property_value(block, "body") {
        let val = body_val.trim();
        if val == "undefined" || val == "null" {
            return None;
        }
    }

    // Determine method — default is GET
    let method = if let Some(method_val) = get_property_value(block, "method") {
        
        method_val
            .trim()
            .trim_matches(|c| c == '\'' || c == '"')
            .to_uppercase()
    } else {
        // If there's a spread element, we can't be sure — skip
        if block.contains("...") {
            return None;
        }
        "GET".to_string()
    };

    if method != "GET" && method != "HEAD" {
        return None;
    }

    Some(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: line_idx + 1,
        column: 1,
        rule_id: "no-invalid-fetch-options".into(),
        message: format!(
            "`body` is not allowed when method is \"{}\".",
            method
        ),
        severity: Severity::Error,
    })
}

/// Check if a property name exists in the block (as an object key).
fn has_property(block: &str, name: &str) -> bool {
    // Look for `name:` or `name :` preceded by a non-alphanumeric char
    let patterns = [
        format!("{name}:"),
        format!("{name} :"),
    ];
    for pat in &patterns {
        if let Some(pos) = block.find(pat.as_str()) {
            // Make sure it's a key, not part of a larger identifier
            if pos == 0 {
                return true;
            }
            let prev = block.as_bytes()[pos - 1];
            if !prev.is_ascii_alphanumeric() && prev != b'_' && prev != b'.' {
                return true;
            }
        }
    }
    false
}

/// Extract the value after `name:` up to the next comma or closing brace.
fn get_property_value(block: &str, name: &str) -> Option<String> {
    let patterns = [
        format!("{name}:"),
        format!("{name} :"),
    ];
    for pat in &patterns {
        if let Some(pos) = block.find(pat.as_str()) {
            // Validate it's a key position
            if pos > 0 {
                let prev = block.as_bytes()[pos - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                    continue;
                }
            }
            let after = &block[pos + pat.len()..];
            let after = after.trim_start();
            // Take until comma, closing brace/paren, or newline
            let end = after
                .find([',', '}', ')'])
                .unwrap_or(after.len());
            return Some(after[..end].trim().to_string());
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        check_source(ctx)
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
    fn flags_body_with_default_get() {
        let code = r#"fetch(url, { body: 'hello' });"#;
        let d = run(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("GET"));
    }

    #[test]
    fn flags_body_with_explicit_get() {
        let code = r#"fetch(url, { method: 'GET', body: 'hello' });"#;
        let d = run(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_body_with_head() {
        let code = r#"fetch(url, { method: 'HEAD', body: 'hello' });"#;
        let d = run(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("HEAD"));
    }

    #[test]
    fn flags_new_request_with_body_get() {
        let code = r#"new Request(url, { body: 'hello' });"#;
        let d = run(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_body_with_post() {
        let code = r#"fetch(url, { method: 'POST', body: JSON.stringify(data) });"#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_body_null() {
        let code = r#"fetch(url, { body: null });"#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_body_undefined() {
        let code = r#"fetch(url, { body: undefined });"#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_spread_without_method() {
        let code = r#"fetch(url, { ...options, body: 'hello' });"#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn flags_multiline_fetch() {
        let code = r#"
fetch(url, {
    body: JSON.stringify(data),
    method: 'GET',
});
"#;
        let d = run(code);
        assert_eq!(d.len(), 1);
    }
}
