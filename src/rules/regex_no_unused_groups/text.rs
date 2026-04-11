use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extracts named group names from a regex pattern.
fn extract_named_groups(line: &str) -> Vec<(String, usize)> {
    let mut groups = Vec::new();
    let pattern = "(?<";
    let mut start = 0;
    while let Some(pos) = line[start..].find(pattern) {
        let abs = start + pos + pattern.len();
        // Make sure this isn't `(?<=` (lookbehind) or `(?<!` (negative lookbehind)
        if abs < line.len() {
            let next = line.as_bytes()[abs];
            if next == b'=' || next == b'!' {
                start = abs;
                continue;
            }
        }
        // Extract group name until `>`
        if let Some(end) = line[abs..].find('>') {
            let name = &line[abs..abs + end];
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                groups.push((name.to_string(), start + pos));
            }
            start = abs + end + 1;
        } else {
            break;
        }
    }
    groups
}

/// Checks if a named group is referenced in surrounding context.
fn is_group_referenced(name: &str, source: &str) -> bool {
    // Check for .groups.name or ["groups"]["name"]
    let dot_access = format!(".groups.{}", name);
    let bracket_access = format!("groups[\"{}\"]", name);
    let bracket_access2 = format!("groups['{}']", name);
    // Check for $<name> in replacement strings
    let replacement_ref = format!("$<{}>", name);

    source.contains(&dot_access)
        || source.contains(&bracket_access)
        || source.contains(&bracket_access2)
        || source.contains(&replacement_ref)
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let groups = extract_named_groups(line);
            for (name, col) in groups {
                if !is_group_referenced(&name, ctx.source) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: "regex-no-unused-groups".into(),
                        message: format!(
                            "Named capturing group `{}` is never referenced \u{2014} use `.groups.{}` or convert to `(?:...)`.",
                            name, name,
                        ),
                        severity: Severity::Warning,
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
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_unused_named_group() {
        let src = r#"const re = /(?<year>\d{4})-(?<month>\d{2})/;"#;
        let diags = run(src);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn allows_used_named_group_dot() {
        let src = "const re = /(?<year>\\d{4})/;\nconst y = match.groups.year;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_used_named_group_replacement() {
        let src = r#"const re = /(?<day>\d{2})/; str.replace(re, "$<day>");"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_lookbehind() {
        // `(?<=...)` is a lookbehind, not a named group
        let src = r#"const re = /(?<=foo)bar/;"#;
        assert!(run(src).is_empty());
    }
}
