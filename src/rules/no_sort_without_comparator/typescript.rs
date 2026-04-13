//! no-sort-without-comparator backend — `.sort()` without comparator.

use crate::diagnostic::{Diagnostic, Severity};

/// `.sort()` or `.sort(  )` — no comparator argument.
fn has_empty_sort(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find(".sort(") {
        let abs = start + pos + 6; // skip past ".sort("
        let rest = &line[abs..];
        let trimmed = rest.trim_start();
        if trimmed.starts_with(')') {
            return true;
        }
        start = abs;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");

    for (idx, line) in text.lines().enumerate() {
        if has_empty_sort(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-sort-without-comparator".into(),
                message: "`.sort()` without comparator sorts lexicographically — pass an explicit compare function.".into(),
                severity: Severity::Error,
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
    fn flags_empty_sort() {
        assert_eq!(run_on("const sorted = arr.sort();").len(), 1);
    }

    #[test]
    fn flags_sort_with_whitespace() {
        assert_eq!(run_on("const sorted = arr.sort(  );").len(), 1);
    }

    #[test]
    fn allows_sort_with_comparator() {
        assert!(run_on("const sorted = arr.sort((a, b) => a - b);").is_empty());
    }
}
