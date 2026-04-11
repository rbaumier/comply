use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// True if the line contains `__dirname` or `__filename` used in string
/// concatenation (`+`) or inside a template literal (`${__dirname}`).
fn has_path_concat(line: &str) -> bool {
    for name in &["__dirname", "__filename"] {
        let mut start = 0;
        while let Some(pos) = line[start..].find(name) {
            let abs = start + pos;
            let after = abs + name.len();

            // Check for `__dirname +` / `__filename +` (with optional spaces).
            if let Some(rest) = line.get(after..) {
                let trimmed = rest.trim_start();
                if trimmed.starts_with('+') {
                    return true;
                }
            }

            // Check for `+ __dirname` / `+ __filename` (preceded by `+`).
            if abs > 0 {
                let before = line[..abs].trim_end();
                if before.ends_with('+') {
                    return true;
                }
            }

            // Check for template literal usage: `${__dirname}` or `${__filename}`.
            if abs >= 2 && line.as_bytes().get(abs - 2) == Some(&b'$')
                && line.as_bytes().get(abs - 1) == Some(&b'{')
            {
                return true;
            }

            start = after;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_path_concat(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "node-no-path-concat".into(),
                    message: "Use `path.join()` or `path.resolve()` instead of string concatenation with `__dirname`/`__filename`.".into(),
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
    fn flags_dirname_plus_string() {
        assert_eq!(run(r#"const p = __dirname + '/foo';"#).len(), 1);
    }

    #[test]
    fn flags_filename_plus_string() {
        assert_eq!(run(r#"const p = __filename + '/bar';"#).len(), 1);
    }

    #[test]
    fn flags_template_literal_dirname() {
        assert_eq!(run(r#"const p = `${__dirname}/foo`;"#).len(), 1);
    }

    #[test]
    fn flags_template_literal_filename() {
        assert_eq!(run(r#"const p = `${__filename}/bar`;"#).len(), 1);
    }

    #[test]
    fn allows_path_join() {
        assert!(run("const p = path.join(__dirname, 'foo');").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// __dirname + '/foo'").is_empty());
    }

    #[test]
    fn flags_string_plus_dirname() {
        assert_eq!(run(r#"const p = '/prefix' + __dirname;"#).len(), 1);
    }
}
