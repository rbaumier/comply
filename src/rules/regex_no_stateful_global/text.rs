use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extracts variable names assigned a `/g` regex: `const foo = /pattern/g` or `let foo = new RegExp("...", "g")`.
fn extract_global_regex_vars(source: &str) -> Vec<(String, usize)> {
    let mut vars = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        // Pattern 1: const/let/var name = /pattern/...g...;
        if let Some(eq_pos) = trimmed.find('=') {
            let lhs = trimmed[..eq_pos].trim();
            let rhs = trimmed[eq_pos + 1..].trim();

            // Extract var name from `const name` / `let name` / `var name`
            let var_name = lhs
                .split_whitespace()
                .last()
                .unwrap_or("");

            if var_name.is_empty() {
                continue;
            }

            // Check RHS for regex literal with /g
            let is_global_literal = is_regex_literal_with_g(rhs);
            // Check RHS for new RegExp("...", "...g...")
            let is_global_constructor = rhs.contains("new RegExp(") && has_g_flag_in_constructor(rhs);

            if is_global_literal || is_global_constructor {
                vars.push((var_name.to_string(), idx + 1));
            }
        }
    }
    vars
}

fn is_regex_literal_with_g(rhs: &str) -> bool {
    // Find `/pattern/flags` and check if flags contain `g`
    if !rhs.starts_with('/') {
        return false;
    }
    // Find closing `/` (skip escaped slashes)
    let bytes = rhs.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'/' {
            // Flags follow
            let flags = &rhs[i + 1..];
            let flag_part: String = flags.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            return flag_part.contains('g');
        }
        i += 1;
    }
    false
}

fn has_g_flag_in_constructor(rhs: &str) -> bool {
    // Look for second argument containing 'g' in new RegExp("...", "...g...")
    if let Some(paren) = rhs.find("new RegExp(") {
        let after = &rhs[paren + 11..];
        // Find the comma separating pattern from flags
        // Simple: count depth for the first argument, then check second
        let mut depth = 0;
        let mut in_string = None;
        for (i, ch) in after.char_indices() {
            match ch {
                '"' | '\'' | '`' if in_string.is_none() => in_string = Some(ch),
                c if in_string == Some(c) => in_string = None,
                '(' if in_string.is_none() => depth += 1,
                ')' if in_string.is_none() && depth > 0 => depth -= 1,
                ')' if in_string.is_none() => return false,
                ',' if in_string.is_none() && depth == 0 => {
                    let second_arg = &after[i + 1..];
                    return second_arg.contains('g');
                }
                _ => {}
            }
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let vars = extract_global_regex_vars(ctx.source);

        for (var_name, def_line) in &vars {
            // Check if this var is used with .test() or .exec()
            for (idx, line) in ctx.source.lines().enumerate() {
                let test_pat = format!("{}.test(", var_name);
                let exec_pat = format!("{}.exec(", var_name);
                if line.contains(&test_pat) || line.contains(&exec_pat) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: *def_line,
                        column: 1,
                        rule_id: "regex-no-stateful-global".into(),
                        message: format!(
                            "Regex `{}` has the `g` flag and is used with `.test()`/`.exec()` (line {}) \u{2014} `lastIndex` is stateful and causes subtle bugs.",
                            var_name,
                            idx + 1,
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
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
    fn flags_global_regex_with_test() {
        let src = "const re = /foo/g;\nif (re.test(str)) {}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("lastIndex"));
    }

    #[test]
    fn flags_global_regex_with_exec() {
        let src = "const re = /bar/gi;\nconst m = re.exec(input);";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_global_regex_without_test_exec() {
        let src = "const re = /foo/g;\nconst result = str.match(re);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_global_regex_with_test() {
        let src = "const re = /foo/i;\nif (re.test(str)) {}";
        assert!(run(src).is_empty());
    }
}
