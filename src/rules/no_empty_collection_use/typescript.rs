//! no-empty-collection-use backend — flag collections read before any
//! element is added.

use crate::diagnostic::{Diagnostic, Severity};

/// Patterns that declare an empty collection.
const EMPTY_PATTERNS: &[&str] = &["= []", "= new Array()", "= new Set()", "= new Map()"];

/// Methods that read/iterate a collection without populating it.
const READ_METHODS: &[&str] = &[
    ".forEach", ".map(", ".filter(", ".find(", ".includes(",
    ".has(", ".get(", ".length", ".size", ".keys(", ".values(",
    ".entries(", ".some(", ".every(", ".reduce(", ".indexOf(",
    "for (", "for(", "of ",
];

/// Methods/operations that populate a collection.
const WRITE_METHODS: &[&str] = &[
    ".push(", ".add(", ".set(", ".unshift(", ".splice(",
    ".concat(", "= [", "= new ",
];

/// Extract variable name from a `const/let/var name = []` declaration.
fn extract_var_name(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("const ")
        .or_else(|| line.strip_prefix("let "))
        .or_else(|| line.strip_prefix("var "))?;
    let eq_pos = rest.find('=')?;
    let name = rest[..eq_pos].trim();
    if name.is_empty() || name.contains('{') || name.contains('[') {
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

crate::ast_check! { |node, source, ctx, diagnostics|
    // Match on the program/module root to do a single-pass scan over lines
    if node.kind() != "program" {
        return;
    }
    let Ok(full_source) = node.utf8_text(source) else { return };
    let lines: Vec<&str> = full_source.lines().collect();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("//") || trimmed.starts_with('*') {
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
            if WRITE_METHODS.iter().any(|w| {
                next.contains(&format!("{name}{w}"))
                    || next.contains(&format!("{name} {}", w.trim_start_matches('.')))
            }) {
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
                    "Collection `{name}` is read before any element is added — this is dead code."
                ),
                severity: Severity::Error,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_empty_array_then_foreach() {
        let src = "const items = [];\nitems.forEach(x => console.log(x));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_empty_set_then_has() {
        let src = "const seen = new Set();\nif (seen.has(key)) {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_push_before_read() {
        let src = "const items = [];\nitems.push(1);\nitems.forEach(x => console.log(x));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_no_immediate_read() {
        let src = "const items = [];\nconst x = 42;\nconsole.log(x);";
        assert!(run_on(src).is_empty());
    }
}
