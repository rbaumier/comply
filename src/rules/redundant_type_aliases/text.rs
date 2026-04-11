use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `type X = Y;` where Y is a single identifier (no unions, intersections, generics, etc.)
fn is_redundant_alias(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with("type ") {
        return false;
    }
    if let Some(eq_pos) = trimmed.find('=') {
        let rhs = trimmed[eq_pos + 1..].trim();
        // Strip trailing semicolon
        let rhs = rhs.trim_end_matches(';').trim();
        if rhs.is_empty() {
            return false;
        }
        // Must be a single identifier: all alphanumeric/underscore, no spaces, no operators
        rhs.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    } else {
        false
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_redundant_alias(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "redundant-type-aliases".into(),
                    message: "Type alias is just renaming — use the original type directly or add structure.".into(),
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
    fn flags_simple_rename() {
        assert_eq!(run("type UserID = string;").len(), 1);
    }

    #[test]
    fn flags_identifier_rename() {
        assert_eq!(run("type Alias = OriginalType;").len(), 1);
    }

    #[test]
    fn allows_union_type() {
        assert!(run("type X = string | number;").is_empty());
    }

    #[test]
    fn allows_intersection_type() {
        assert!(run("type X = A & B;").is_empty());
    }

    #[test]
    fn allows_generic_type() {
        assert!(run("type X = Array<string>;").is_empty());
    }

    #[test]
    fn allows_object_type() {
        assert!(run("type X = { name: string };").is_empty());
    }
}
