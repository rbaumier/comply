//! no-invalid-fetch-options AST backend — flag `fetch()`/`new Request()` with
//! `body` on GET or HEAD requests.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a property name exists in the block (as an object key).
fn has_property(block: &str, name: &str) -> bool {
    let patterns = [format!("{name}:"), format!("{name} :")];
    for pat in &patterns {
        if let Some(pos) = block.find(pat.as_str()) {
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
    let patterns = [format!("{name}:"), format!("{name} :")];
    for pat in &patterns {
        if let Some(pos) = block.find(pat.as_str()) {
            if pos > 0 {
                let prev = block.as_bytes()[pos - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                    continue;
                }
            }
            let after = &block[pos + pat.len()..];
            let after = after.trim_start();
            let end = after.find([',', '}', ')']).unwrap_or(after.len());
            return Some(after[..end].trim().to_string());
        }
    }
    None
}

/// Analyze a `fetch(…)` or `new Request(…)` call block for body + method.
fn analyze_call_block(
    block: &str,
    ctx: &crate::rules::backend::CheckCtx,
    line_idx: usize,
) -> Option<Diagnostic> {
    let has_body = has_property(block, "body");
    if !has_body {
        return None;
    }

    if let Some(body_val) = get_property_value(block, "body") {
        let val = body_val.trim();
        if val == "undefined" || val == "null" {
            return None;
        }
    }

    let method = if let Some(method_val) = get_property_value(block, "method") {
        method_val
            .trim()
            .trim_matches(|c| c == '\'' || c == '"')
            .to_uppercase()
    } else {
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
        message: format!("`body` is not allowed when method is \"{}\".", method),
        severity: Severity::Error,
        span: None,
    })
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let is_fetch = line.contains("fetch(")
            && !line.contains("fetchOptions")
            && !line.contains("fetcher");
        let is_request = line.contains("new Request(");

        if !is_fetch && !is_request {
            i += 1;
            continue;
        }

        let start_line = i;
        let trigger = if is_fetch { "fetch(" } else { "new Request(" };
        let trigger_pos = line.find(trigger).unwrap();
        let mut depth: i32 = 0;
        let mut block = String::new();

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

        if let Some(diag) = analyze_call_block(&block, ctx, start_line) {
            diagnostics.push(diag);
        }

        i = j.max(i + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_body_with_default_get() {
        let code = r#"fetch(url, { body: 'hello' });"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("GET"));
    }

    #[test]
    fn flags_body_with_explicit_get() {
        let code = r#"fetch(url, { method: 'GET', body: 'hello' });"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_body_with_head() {
        let code = r#"fetch(url, { method: 'HEAD', body: 'hello' });"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("HEAD"));
    }

    #[test]
    fn flags_new_request_with_body_get() {
        let code = r#"new Request(url, { body: 'hello' });"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_body_with_post() {
        let code = r#"fetch(url, { method: 'POST', body: JSON.stringify(data) });"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_body_null() {
        let code = r#"fetch(url, { body: null });"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_body_undefined() {
        let code = r#"fetch(url, { body: undefined });"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_spread_without_method() {
        let code = r#"fetch(url, { ...options, body: 'hello' });"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_multiline_fetch() {
        let code = r#"
fetch(url, {
    body: JSON.stringify(data),
    method: 'GET',
});
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }
}
