use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

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
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(source) = extract_import_source(line)
                .filter(|s| s.starts_with('/'))
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-absolute-path".into(),
                    message: format!(
                        "Do not import modules using an absolute path (`{}`).",
                        source
                    ),
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
    fn flags_absolute_import() {
        let src = "import { foo } from '/usr/lib/utils';\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("absolute path"));
    }

    #[test]
    fn allows_relative_import() {
        let src = "import { foo } from './utils';\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_package_import() {
        let src = "import { foo } from 'lodash';\n";
        assert!(run(src).is_empty());
    }
}
