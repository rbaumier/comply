//! require-module-specifiers backend — flag import/export statements with
//! empty specifier lists.
//!
//! Matches patterns like:
//! - `import {} from './module'`
//! - `export {} from './module'`
//! - `import {  } from './module'`
//!
//! Does NOT flag side-effect imports (`import './module'`) — those are
//! intentional and have no specifier braces at all.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Check if a line has an import/export with empty specifier braces `{}`.
fn has_empty_specifiers(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();

    let (stmt_type, after_keyword) = if let Some(rest) = trimmed.strip_prefix("import ") {
        // Skip `import type` — treated separately below
        if rest.trim_start().starts_with("type ") {
            let after_type = rest.trim_start().strip_prefix("type ")?;
            return check_empty_braces_before_from(after_type).map(|_| "import");
        }
        ("import", rest)
    } else if let Some(rest) = trimmed.strip_prefix("export ") {
        // Skip `export type` — same handling
        if rest.trim_start().starts_with("type ") {
            let after_type = rest.trim_start().strip_prefix("type ")?;
            return check_empty_braces_before_from(after_type).map(|_| "export");
        }
        ("export", rest)
    } else {
        return None;
    };

    check_empty_braces_before_from(after_keyword).map(|_| stmt_type)
}

/// Check if text starts with `{ }` (with optional whitespace inside)
/// followed by `from`.
fn check_empty_braces_before_from(text: &str) -> Option<()> {
    let text = text.trim_start();
    if !text.starts_with('{') {
        return None;
    }
    let after_open = text[1..].trim_start();
    if !after_open.starts_with('}') {
        return None;
    }
    let after_close = after_open[1..].trim_start();
    if after_close.starts_with("from ")
        || after_close.starts_with("from\"")
        || after_close.starts_with("from'")
    {
        Some(())
    } else {
        None
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        ctx.source
            .lines()
            .enumerate()
            .filter_map(|(idx, line)| {
                let stmt_type = has_empty_specifiers(line)?;
                Some(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "require-module-specifiers".into(),
                    message: format!(
                        "{stmt_type} statement with empty specifiers `{{}}` is not \
                         allowed — add specifiers, use a side-effect import, or \
                         remove the statement."
                    ),
                    severity: Severity::Warning,
                })
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

    #[test]
    fn flags_import_with_empty_specifiers() {
        let diags = run("import {} from './module';");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("import"));
    }

    #[test]
    fn flags_export_with_empty_specifiers() {
        let diags = run("export {} from './module';");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("export"));
    }

    #[test]
    fn flags_spaced_empty_specifiers() {
        let diags = run("import {  } from './module';");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_import_with_specifiers() {
        assert!(run("import { foo } from './module';").is_empty());
    }

    #[test]
    fn allows_side_effect_import() {
        assert!(run("import './module';").is_empty());
    }

    #[test]
    fn allows_default_import() {
        assert!(run("import foo from './module';").is_empty());
    }

    #[test]
    fn allows_namespace_import() {
        assert!(run("import * as mod from './module';").is_empty());
    }

    #[test]
    fn allows_export_with_specifiers() {
        assert!(run("export { foo, bar } from './module';").is_empty());
    }

    #[test]
    fn flags_type_import_with_empty_specifiers() {
        let diags = run("import type {} from './module';");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_regular_code() {
        assert!(run("const x = {};").is_empty());
    }
}
