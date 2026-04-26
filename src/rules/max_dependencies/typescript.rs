//! max-dependencies backend — count unique import sources at the top of the
//! file and flag when the count exceeds the configured threshold.
//!
//! Walk the `program` root once, count distinct `import_statement` source
//! specifiers (so duplicates don't inflate the tally), and emit a single
//! diagnostic anchored on the last import when the limit is breached.

use std::collections::HashSet;

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let max = ctx.config.threshold("max-dependencies", "max");
    let mut seen: HashSet<String> = HashSet::new();
    let mut last_import_line: usize = 1;

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "import_statement" {
            continue;
        }
        let Some(source_node) = child.child_by_field_name("source") else {
            continue;
        };
        let Ok(spec) = std::str::from_utf8(&source[source_node.byte_range()]) else {
            continue;
        };
        let spec_clean = spec
            .trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .to_string();
        seen.insert(spec_clean);
        last_import_line = child.start_position().row + 1;
    }

    if seen.len() > max {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: last_import_line,
            column: 1,
            rule_id: "max-dependencies".into(),
            message: format!(
                "Maximum number of dependencies ({}) exceeded — this file imports {} modules.",
                max,
                seen.len()
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_too_many_imports() {
        let mut src = String::new();
        for i in 0..16 {
            src.push_str(&format!("import {{ x{i} }} from 'mod-{i}';\n"));
        }
        let diags = run_on(&src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("16 modules"));
    }

    #[test]
    fn allows_within_limit() {
        let mut src = String::new();
        for i in 0..15 {
            src.push_str(&format!("import {{ x{i} }} from 'mod-{i}';\n"));
        }
        assert!(run_on(&src).is_empty());
    }

    #[test]
    fn deduplicates_same_module() {
        let mut src = String::new();
        for i in 0..16 {
            src.push_str(&format!("import {{ x{i} }} from 'same-mod';\n"));
        }
        assert!(run_on(&src).is_empty());
    }
}
