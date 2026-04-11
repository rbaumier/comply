//! no-empty-collection-use Rust backend.
//!
//! Flag collections read before any element is added.
//! Detects `Vec::new()`, `HashMap::new()`, `HashSet::new()`, `BTreeMap::new()`,
//! `BTreeSet::new()`, `vec![]` followed by reads without writes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

const EMPTY_PATTERNS: &[&str] = &[
    "Vec::new()",
    "HashMap::new()",
    "HashSet::new()",
    "BTreeMap::new()",
    "BTreeSet::new()",
    "VecDeque::new()",
    "vec![]",
];

const READ_METHODS: &[&str] = &[
    ".iter()", ".into_iter()", ".len()", ".is_empty()",
    ".contains(", ".get(", ".first()", ".last()",
    ".keys()", ".values()", ".entries(",
    "for ",
];

const WRITE_METHODS: &[&str] = &[
    ".push(", ".insert(", ".extend(", ".append(",
    ".push_back(", ".push_front(",
];

fn extract_var_name(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("let mut ")
        .or_else(|| line.strip_prefix("let "))?;
    let eq_pos = rest.find('=')?;
    let name = rest[..eq_pos].trim();
    if name.is_empty() || name.contains('{') || name.contains('(') {
        return None;
    }
    if let Some(colon_pos) = name.find(':') {
        let n = name[..colon_pos].trim();
        if n.is_empty() {
            return None;
        }
        return Some(n.to_string());
    }
    Some(name.to_string())
}

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }

            let is_empty_decl = EMPTY_PATTERNS.iter().any(|p| trimmed.contains(p));
            if !is_empty_decl {
                continue;
            }

            let Some(name) = extract_var_name(trimmed) else { continue };

            let mut found_write = false;
            let mut found_read = false;

            for next_line in lines.iter().take(lines.len().min(idx + 6)).skip(idx + 1) {
                let next = next_line.trim();
                if next.is_empty() || next.starts_with("//") {
                    continue;
                }
                if !next.contains(name.as_str()) {
                    continue;
                }
                if WRITE_METHODS.iter().any(|w| next.contains(&format!("{name}{w}"))) {
                    found_write = true;
                    break;
                }
                if READ_METHODS.iter().any(|r| {
                    next.contains(&format!("{name}{r}"))
                        || (r.starts_with("for") && next.contains(name.as_str()))
                }) {
                    found_read = true;
                    break;
                }
            }

            if !found_write && found_read {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-empty-collection-use".into(),
                    message: format!(
                        "Collection `{name}` is read before any element is added \u{2014} this is dead code."
                    ),
                    severity: Severity::Error,
                });
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_empty_vec_then_iter() {
        let src = "let items = Vec::new();\nfor x in items.iter() {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_push_before_read() {
        let src = "let mut items = Vec::new();\nitems.push(1);\nfor x in items.iter() {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_no_immediate_read() {
        let src = "let items = Vec::new();\nlet x = 42;\nprintln!(\"{}\", x);";
        assert!(run_on(src).is_empty());
    }
}
