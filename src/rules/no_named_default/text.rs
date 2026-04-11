//! no-named-default backend — flag `import { default as foo }` patterns.
//!
//! The named form `{ default as foo }` is verbose and obscures intent.
//! The idiomatic form is `import foo from './m'`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Check if a line contains `{ default as <name> }` in an import statement.
/// Returns the alias name if found.
fn find_named_default_import(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("import") {
        return None;
    }

    // Look for `default as <name>` inside braces.
    let open = trimmed.find('{')?;
    let close = trimmed.find('}')?;
    if close <= open {
        return None;
    }

    let names_str = &trimmed[open + 1..close];
    for part in names_str.split(',') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("default as ") {
            let alias = rest.trim();
            if !alias.is_empty() {
                return Some(alias.to_string());
            }
        }
        // Also handle `default as` with extra spaces.
        if let Some(rest) = part.strip_prefix("default") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix("as") {
                let rest = rest.trim_start();
                if !rest.is_empty() {
                    return Some(rest.to_string());
                }
            }
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if let Some(alias) = find_named_default_import(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-named-default".into(),
                    message: format!(
                        "Replace `{{ default as {alias} }}` with `import {alias} from …` \
                         — prefer the default import syntax."
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
    fn flags_named_default_import() {
        let d = run(r#"import { default as foo } from './m';"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import foo from"));
    }

    #[test]
    fn flags_named_default_with_others() {
        let d = run(r#"import { default as foo, bar } from './m';"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("foo"));
    }

    #[test]
    fn allows_regular_default_import() {
        assert!(run(r#"import foo from './m';"#).is_empty());
    }

    #[test]
    fn allows_named_imports() {
        assert!(run(r#"import { bar, baz } from './m';"#).is_empty());
    }

    #[test]
    fn allows_default_keyword_in_non_import() {
        assert!(run("const x = { default: 1 };").is_empty());
    }

    #[test]
    fn skips_comments() {
        assert!(run(r#"// import { default as foo } from './m';"#).is_empty());
    }
}
