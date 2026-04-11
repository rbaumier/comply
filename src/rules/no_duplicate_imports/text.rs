use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashMap;

#[derive(Debug)]
pub struct Check;

/// Extract the module source from an import line, e.g. `import { a } from 'x'` → `x`.
fn extract_import_source(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with("import ") {
        return None;
    }
    // Skip type-only re-exports like `import type { X } from 'y'` — still same module.
    // Find `from '...'` or `from "..."`.
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
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let line_num = idx + 1;
            if let Some(source) = extract_import_source(line) {
                let source_owned = source.to_string();
                if let Some(&first_line) = seen.get(&source_owned) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: line_num,
                        column: 1,
                        rule_id: "no-duplicate-imports".into(),
                        message: format!(
                            "Duplicate import from `{}` — already imported on line {}. Merge into a single statement.",
                            source, first_line
                        ),
                        severity: Severity::Warning,
                    });
                } else {
                    seen.insert(source_owned, line_num);
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
    fn flags_duplicate_imports() {
        let src = r#"import { a } from 'x';
import { b } from 'x';
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Duplicate import from `x`"));
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn allows_distinct_sources() {
        let src = r#"import { a } from 'x';
import { b } from 'y';
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_three_duplicates() {
        let src = r#"import { a } from 'lodash';
import { b } from 'lodash';
import { c } from 'lodash';
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 2);
    }
}
