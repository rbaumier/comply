//! no-unused-collection AST backend — collection populated but never read.

use crate::diagnostic::{Diagnostic, Severity};

/// Collection constructor patterns on the right side of `const x = ...`.
const COLLECTION_CONSTRUCTORS: &[&str] = &[
    "[]",
    "new Map(",
    "new Set(",
    "new Array(",
    "new WeakMap(",
    "new WeakSet(",
];

/// Mutation methods — indicate the collection is written to.
const WRITE_METHODS: &[&str] = &[".push(", ".add(", ".set(", ".unshift(", ".splice("];

/// Read patterns — indicate the collection value is consumed.
const READ_METHODS: &[&str] = &[
    ".forEach(",
    ".map(",
    ".filter(",
    ".find(",
    ".some(",
    ".every(",
    ".reduce(",
    ".includes(",
    ".indexOf(",
    ".get(",
    ".has(",
    ".keys(",
    ".values(",
    ".entries(",
    ".join(",
    ".flat(",
    ".flatMap(",
    ".slice(",
    ".length",
    ".size",
    "[",
];

/// Extract the variable name from `const <name> = <constructor>`.
fn extract_collection_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("const ")?;
    let eq_pos = rest.find('=')?;
    let name = rest[..eq_pos].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
        return None;
    }
    let rhs = rest[eq_pos + 1..].trim();
    let is_collection = COLLECTION_CONSTRUCTORS.iter().any(|c| rhs.starts_with(c));
    if is_collection {
        Some(name.to_string())
    } else {
        None
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = text.lines().collect();

    // First pass: find all collection declarations.
    let mut collections: Vec<(String, usize)> = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if let Some(name) = extract_collection_name(line) {
            collections.push((name, idx));
        }
    }

    // Second pass: for each collection, check if it's written but never read.
    for (name, decl_line) in &collections {
        let mut is_written = false;
        let mut is_read = false;

        for (idx, line) in lines.iter().enumerate() {
            if idx == *decl_line {
                continue;
            }
            if !line.contains(name.as_str()) {
                continue;
            }

            for wm in WRITE_METHODS {
                let pattern = format!("{name}{wm}");
                if line.contains(&pattern) {
                    is_written = true;
                }
            }

            for rm in READ_METHODS {
                let pattern = format!("{name}{rm}");
                if line.contains(&pattern) {
                    is_read = true;
                }
            }

            let trimmed = line.trim();

            if trimmed.starts_with("return ") && trimmed.contains(name.as_str()) {
                is_read = true;
            }

            let spread = format!("...{name}");
            if line.contains(&spread) {
                is_read = true;
            }

            let call_pattern = format!("({name})");
            let call_pattern2 = format!("({name},");
            let call_pattern3 = format!(", {name})");
            let call_pattern4 = format!(", {name},");
            if line.contains(&call_pattern)
                || line.contains(&call_pattern2)
                || line.contains(&call_pattern3)
                || line.contains(&call_pattern4)
            {
                is_read = true;
            }

            let for_of = format!("of {name}");
            if line.contains(&for_of) {
                is_read = true;
            }
        }

        if is_written && !is_read {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: decl_line + 1,
                column: 1,
                rule_id: "no-unused-collection".into(),
                message: format!(
                    "Collection `{name}` is populated but never read."
                ),
                severity: Severity::Warning,
                span: None,
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
    fn flags_pushed_but_never_read() {
        let src = r#"
const items = [];
items.push(1);
items.push(2);
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_set_add_but_never_read() {
        let src = r#"
const seen = new Set();
seen.add("a");
seen.add("b");
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_pushed_and_iterated() {
        let src = r#"
const items = [];
items.push(1);
items.forEach(x => console.log(x));
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_pushed_and_returned() {
        let src = r#"
const items = [];
items.push(1);
return items;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_collection_passed_as_arg() {
        let src = r#"
const items = [];
items.push(1);
doSomething(items);
"#;
        assert!(run_on(src).is_empty());
    }
}
