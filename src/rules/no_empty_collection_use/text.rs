use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Skip comments
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }

            // Check if line declares an empty collection
            let is_empty_decl = EMPTY_PATTERNS.iter().any(|p| trimmed.contains(p));
            if !is_empty_decl {
                continue;
            }

            // Extract the variable name
            let Some(name) = extract_var_name(trimmed) else {
                continue;
            };

            // Scan forward up to 5 non-blank lines looking for read vs write
            let mut found_write = false;
            let mut found_read = false;

            for next_line in lines.iter().take(lines.len().min(idx + 6)).skip(idx + 1) {
                let next = next_line.trim();
                if next.is_empty() || next.starts_with("//") {
                    continue;
                }

                if !next.contains(name) {
                    continue;
                }

                // Check if this line populates the collection
                if WRITE_METHODS.iter().any(|w| next.contains(&format!("{name}{w}")) || next.contains(&format!("{name} {}", w.trim_start_matches('.')))) {
                    found_write = true;
                    break;
                }

                // Check if this line reads the collection
                if READ_METHODS.iter().any(|r| next.contains(&format!("{name}{r}")) || (r.starts_with("for") && next.contains(name))) {
                    found_read = true;
                    break;
                }
            }

            if !found_write
                && found_read {
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

        diagnostics
    }
}

/// Extract variable name from a `const/let/var name = []` declaration.
fn extract_var_name(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("const ")
        .or_else(|| line.strip_prefix("let "))
        .or_else(|| line.strip_prefix("var "))?;
    let eq_pos = rest.find('=')?;
    let name = rest[..eq_pos].trim();
    // Reject destructuring or type annotations with complex syntax
    if name.is_empty() || name.contains('{') || name.contains('[') {
        return None;
    }
    // Strip type annotation: `name: Type[]` -> `name`
    if let Some(colon_pos) = name.find(':') {
        let n = name[..colon_pos].trim();
        if n.is_empty() {
            return None;
        }
        return Some(n);
    }
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_empty_array_then_foreach() {
        let src = "const items = [];\nitems.forEach(x => console.log(x));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_set_then_has() {
        let src = "const seen = new Set();\nif (seen.has(key)) {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_map_then_get() {
        let src = "const cache = new Map();\nconst val = cache.get('key');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_push_before_read() {
        let src = "const items = [];\nitems.push(1);\nitems.forEach(x => console.log(x));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_populated_set() {
        let src = "const seen = new Set();\nseen.add(key);\nif (seen.has(other)) {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_immediate_read() {
        let src = "const items = [];\nconst x = 42;\nconsole.log(x);";
        assert!(run(src).is_empty());
    }
}
