use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract (object_name) from a `const/let VAR = OBJ.prop;` line.
/// Returns the object name if the line matches the pattern.
fn extract_object_access(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("const ")
        .or_else(|| trimmed.strip_prefix("let "))?;
    let rest = rest.trim_start();
    // Skip the variable name
    let eq_pos = rest.find('=')?;
    let after_eq = rest[eq_pos + 1..].trim_start();
    // Must contain a dot access: OBJ.prop
    let dot_pos = after_eq.find('.')?;
    let obj = after_eq[..dot_pos].trim();
    // Object name must be a simple identifier
    if obj.is_empty()
        || !obj
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
    {
        return None;
    }
    // After the dot there should be a property name (not a method call with complex args)
    let after_dot = &after_eq[dot_pos + 1..];
    let prop_end = after_dot
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
        .unwrap_or(after_dot.len());
    if prop_end == 0 {
        return None;
    }
    // Make sure it ends with `;` (or `) or similar — not a method call
    let remainder = after_dot[prop_end..].trim();
    if remainder.starts_with('(') {
        // This is a method call, not a property access
        return None;
    }
    Some(obj)
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            if let Some(obj) = extract_object_access(lines[i]) {
                let start = i;
                let mut count = 1;
                let mut j = i + 1;
                while j < lines.len() {
                    if let Some(next_obj) = extract_object_access(lines[j]) {
                        if next_obj == obj {
                            count += 1;
                            j += 1;
                            continue;
                        }
                    }
                    break;
                }
                if count >= 2 {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: start + 1,
                        column: 1,
                        rule_id: "prefer-destructuring-assignment".into(),
                        message: format!(
                            "{count} consecutive property accesses on `{obj}` — use destructuring instead."
                        ),
                        severity: Severity::Warning,
                    });
                    i = j;
                    continue;
                }
            }
            i += 1;
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
    fn flags_consecutive_accesses() {
        let src = "const x = obj.x;\nconst y = obj.y;";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("obj"));
    }

    #[test]
    fn flags_three_consecutive() {
        let src = "const a = config.a;\nconst b = config.b;\nconst c = config.c;";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("3"));
    }

    #[test]
    fn allows_single_access() {
        assert!(run("const x = obj.x;").is_empty());
    }

    #[test]
    fn allows_different_objects() {
        let src = "const x = obj1.x;\nconst y = obj2.y;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_method_calls() {
        let src = "const x = obj.getX();\nconst y = obj.getY();";
        assert!(run(src).is_empty());
    }
}
