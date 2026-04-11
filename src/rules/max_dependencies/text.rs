use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashSet;

#[derive(Debug)]
pub struct Check;

const DEFAULT_MAX: usize = 15;

/// Extract the module source from an import line.
fn extract_import_source(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with("import ") {
        return None;
    }
    let from_idx = trimmed.rfind(" from ")?;
    let rest = trimmed[from_idx + 6..].trim().trim_end_matches(';');
    let rest = rest.trim();
    if (rest.starts_with('\'') && rest.ends_with('\''))
        || (rest.starts_with('"') && rest.ends_with('"'))
    {
        Some(&rest[1..rest.len() - 1])
    } else {
        None
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let max = ctx.config.threshold("max-dependencies", "max", DEFAULT_MAX);

        let mut deps: HashSet<String> = HashSet::new();
        let mut last_import_line: usize = 1;

        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(source) = extract_import_source(line) {
                deps.insert(source.to_string());
                last_import_line = idx + 1;
            }
        }

        if deps.len() > max {
            return vec![Diagnostic {
                path: ctx.path.to_path_buf(),
                line: last_import_line,
                column: 1,
                rule_id: "max-dependencies".into(),
                message: format!(
                    "Maximum number of dependencies ({}) exceeded — this file imports {} modules.",
                    max,
                    deps.len()
                ),
                severity: Severity::Warning,
            }];
        }

        Vec::new()
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
    fn flags_too_many_imports() {
        let mut src = String::new();
        for i in 0..16 {
            src.push_str(&format!("import {{ x{i} }} from 'mod-{i}';\n"));
        }
        let diags = run(&src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("16 modules"));
    }

    #[test]
    fn allows_within_limit() {
        let mut src = String::new();
        for i in 0..15 {
            src.push_str(&format!("import {{ x{i} }} from 'mod-{i}';\n"));
        }
        assert!(run(&src).is_empty());
    }

    #[test]
    fn deduplicates_same_module() {
        let mut src = String::new();
        for i in 0..16 {
            // All import from the same module
            src.push_str(&format!("import {{ x{i} }} from 'same-mod';\n"));
        }
        assert!(run(&src).is_empty()); // Only 1 unique dep
    }
}
