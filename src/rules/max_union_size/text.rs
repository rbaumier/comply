use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Return true if the line contains a union type with 5+ `|` separators.
fn has_large_union(line: &str) -> bool {
    let trimmed = line.trim();

    // type alias: `type X = A | B | C | D | E | F`
    if trimmed.starts_with("type ")
        && let Some(eq_pos) = trimmed.find('=') {
            let rhs = &trimmed[eq_pos + 1..];
            let pipes = rhs.chars().filter(|&c| c == '|').count();
            return pipes >= 5;
        }

    // type annotation after `:` — e.g. `param: A | B | C | D | E | F`
    if let Some(colon_pos) = trimmed.find(':') {
        let after_colon = &trimmed[colon_pos + 1..];
        let pipes = after_colon.chars().filter(|&c| c == '|').count();
        if pipes >= 5 {
            return true;
        }
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_large_union(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "max-union-size".into(),
                    message: "Union type has more than 5 members — consider extracting a type alias."
                        .into(),
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
    fn flags_large_union_in_type_alias() {
        let src = "type Status = A | B | C | D | E | F;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_large_union_in_annotation() {
        let src = "function foo(x: A | B | C | D | E | F) {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_small_union() {
        let src = "type Status = A | B | C;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_four_pipes() {
        let src = "type X = A | B | C | D | E;";
        assert!(run(src).is_empty());
    }
}
