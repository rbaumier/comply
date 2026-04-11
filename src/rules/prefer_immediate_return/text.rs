use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        // Collect non-blank line indices for pairwise scanning.
        let non_blank: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .map(|(i, _)| i)
            .collect();

        for pair in non_blank.windows(2) {
            let (i, j) = (pair[0], pair[1]);
            let assign_line = lines[i].trim();
            let return_line = lines[j].trim();

            // Skip comments
            if assign_line.starts_with("//") || assign_line.starts_with('*') {
                continue;
            }

            // Match: (const|let|var) <name> = ...;
            let name = extract_assigned_name(assign_line);
            if let Some(name) = name {
                // Match: return <name>;
                let expected = format!("return {};", name);
                if return_line == expected || return_line == format!("return {name}") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: 1,
                        rule_id: "prefer-immediate-return".into(),
                        message: format!(
                            "Variable `{name}` is assigned and immediately returned — return the expression directly."
                        ),
                        severity: Severity::Warning,
                    });
                }
            }
        }

        diagnostics
    }
}

/// Extract the identifier name from a `const/let/var name = ...;` line.
fn extract_assigned_name(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("const ")
        .or_else(|| line.strip_prefix("let "))
        .or_else(|| line.strip_prefix("var "))?;
    // Take the identifier (word chars) before `=`
    let eq_pos = rest.find('=')?;
    let name = rest[..eq_pos].trim();
    // Must be a simple identifier (no destructuring)
    if name.is_empty()
        || name.contains('{')
        || name.contains('[')
        || name.contains(':')
        || name.contains(' ')
    {
        return None;
    }
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_assign_then_return() {
        let src = "const result = computeValue();\nreturn result;";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 1);
    }

    #[test]
    fn flags_let_assign_then_return() {
        let src = "let x = a + b;\nreturn x;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_assign_used_later() {
        let src = "const result = computeValue();\nconsole.log(result);\nreturn result;";
        // The assign is not immediately followed by return (there's a console.log in between)
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_different_variable_returned() {
        let src = "const result = computeValue();\nreturn other;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_destructuring() {
        let src = "const { a, b } = getValues();\nreturn a;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_with_blank_line_between() {
        let src = "const result = computeValue();\n\nreturn result;";
        assert_eq!(run(src).len(), 1);
    }
}
