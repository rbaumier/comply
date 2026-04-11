use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct Check;

/// Matches `.indexOf(`, `.lastIndexOf(`, `.findIndex(`, `.findLastIndex(`
static INDEX_METHOD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\.(indexOf|lastIndexOf|findIndex|findLastIndex)\s*\(").expect("valid regex")
});

/// Matches patterns like `identifier < 0`, `identifier >= 0`, `identifier > -1`
/// which are inconsistent index existence checks.
static BAD_COMPARISON_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        (\w+)              # identifier (capture group 1)
        \s*
        (                  # operator + value (capture group 2)
          <\s*0            #   < 0
        | >=\s*0           #   >= 0
        | >\s*-\s*1        #   > -1
        )
        (?:\s|[);,}]|$)    # followed by whitespace, delimiter, or end
    ",
    )
    .expect("valid regex")
});

fn find_bad_index_checks(source: &str) -> Vec<(usize, String)> {
    let mut results = Vec::new();
    let mut index_vars: Vec<String> = Vec::new();

    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
            continue;
        }

        // Track: const index = foo.indexOf(...)
        if let Some(eq_pos) = trimmed.find('=')
            && eq_pos > 0
                && trimmed.as_bytes().get(eq_pos + 1) != Some(&b'=')
                && (eq_pos == 0 || trimmed.as_bytes()[eq_pos - 1] != b'!')
            {
                let before_eq = trimmed[..eq_pos].trim();
                let after_eq = trimmed[eq_pos + 1..].trim();

                let var_name = before_eq
                    .strip_prefix("const ")
                    .or_else(|| before_eq.strip_prefix("let "))
                    .or_else(|| before_eq.strip_prefix("var "))
                    .unwrap_or(before_eq)
                    .trim();

                if is_valid_identifier(var_name) && INDEX_METHOD_RE.is_match(after_eq) {
                    index_vars.push(var_name.to_string());
                }
            }

        // Check for inline bad comparisons: foo.indexOf('x') < 0
        if INDEX_METHOD_RE.is_match(trimmed) && has_inline_bad_comparison(trimmed) {
            results.push((idx + 1, build_message(trimmed)));
            continue;
        }

        // Check for variable-based bad comparisons: index < 0
        if !index_vars.is_empty()
            && let Some(caps) = BAD_COMPARISON_RE.captures(trimmed) {
                let var_name = caps.get(1).unwrap().as_str();
                if index_vars.iter().any(|v| v == var_name) {
                    let operator = caps.get(2).unwrap().as_str().trim();
                    results.push((idx + 1, build_message_for_operator(operator)));
                }
            }
    }

    results
}

fn is_valid_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Check for inline patterns like `foo.indexOf('x') < 0`
fn has_inline_bad_comparison(line: &str) -> bool {
    if let Some(m) = INDEX_METHOD_RE.find(line) {
        let after_method = &line[m.end()..];
        if let Some(close) = find_closing_paren(after_method) {
            let rest = after_method[close + 1..].trim_start();
            return rest.starts_with("< 0")
                || rest.starts_with("<0")
                || rest.starts_with(">= 0")
                || rest.starts_with(">=0")
                || rest.starts_with("> -1")
                || rest.starts_with(">-1")
                || rest.starts_with("> - 1");
        }
    }
    false
}

/// Simple paren balancer — returns index of closing ')' relative to start.
fn find_closing_paren(s: &str) -> Option<usize> {
    let mut depth = 1i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn build_message(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.contains("< 0") || trimmed.contains("<0") {
        "Prefer `=== -1` over `< 0` to check index non-existence.".into()
    } else if trimmed.contains(">= 0") || trimmed.contains(">=0") {
        "Prefer `!== -1` over `>= 0` to check index existence.".into()
    } else {
        "Prefer `!== -1` over `> -1` to check index existence.".into()
    }
}

fn build_message_for_operator(op: &str) -> String {
    let normalized: String = op.chars().filter(|c| !c.is_whitespace()).collect();
    match normalized.as_str() {
        "<0" => "Prefer `=== -1` over `< 0` to check index non-existence.".into(),
        ">=0" => "Prefer `!== -1` over `>= 0` to check index existence.".into(),
        _ => "Prefer `!== -1` over `> -1` to check index existence.".into(),
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        find_bad_index_checks(ctx.source)
            .into_iter()
            .map(|(line, message)| Diagnostic {
                path: ctx.path.to_path_buf(),
                line,
                column: 1,
                rule_id: "consistent-existence-index-check".into(),
                message,
                severity: Severity::Warning,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    // --- Bad patterns (should flag) ---

    #[test]
    fn flags_inline_index_of_less_than_zero() {
        let d = run("if (foo.indexOf('bar') < 0) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("=== -1"));
    }

    #[test]
    fn flags_inline_index_of_gte_zero() {
        let d = run("if (foo.indexOf('bar') >= 0) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!== -1"));
    }

    #[test]
    fn flags_inline_index_of_gt_minus_one() {
        let d = run("if (foo.indexOf('bar') > -1) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!== -1"));
    }

    #[test]
    fn flags_variable_less_than_zero() {
        let src = "const idx = arr.indexOf('x');\nif (idx < 0) {}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 2);
    }

    #[test]
    fn flags_variable_gte_zero() {
        let src = "const idx = arr.findIndex(x => x > 0);\nif (idx >= 0) {}";
        let d = run(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_find_last_index() {
        let d = run("if (arr.findLastIndex(x => x) > -1) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_last_index_of() {
        let d = run("if (str.lastIndexOf('a') < 0) {}");
        assert_eq!(d.len(), 1);
    }

    // --- Good patterns (should not flag) ---

    #[test]
    fn allows_triple_equals_minus_one() {
        assert!(run("if (foo.indexOf('bar') === -1) {}").is_empty());
    }

    #[test]
    fn allows_not_equals_minus_one() {
        assert!(run("if (foo.indexOf('bar') !== -1) {}").is_empty());
    }

    #[test]
    fn allows_variable_triple_equals() {
        let src = "const idx = arr.indexOf('x');\nif (idx === -1) {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unrelated_comparison() {
        assert!(run("if (count < 0) {}").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// foo.indexOf('bar') < 0").is_empty());
    }
}
