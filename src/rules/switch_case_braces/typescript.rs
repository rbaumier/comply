//! switch-case-braces backend — flag `case` clauses whose body contains
//! statements but is not wrapped in a block `{ }`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // We look for `switch_case` (and `switch_default`) nodes.
    let is_case = node.kind() == "switch_case";
    let is_default = node.kind() == "switch_default";
    if !is_case && !is_default {
        return;
    }

    // Count the child statements (named children that are NOT the `value`
    // field). In tree-sitter's TypeScript grammar, a `switch_case` has:
    //   - a `value` field (the case expression) — only for `switch_case`
    //   - zero or more statement children (the body)
    //
    // If there's exactly one child that is a `statement_block`, braces
    // are already present. If there are zero statement children (fall-through),
    // no braces are needed. Otherwise, flag.

    let mut stmt_count = 0;
    let mut has_block = false;
    let child_count = node.named_child_count();

    for i in 0..child_count {
        let child = node.named_child(i).unwrap();
        // Skip the `value` field on `switch_case`
        if is_case && i == 0 && node.field_name_for_child(child.id() as u32).is_none() {
            // Actually, let's check by field name
        }

        // In tree-sitter, the case value is the first named child for switch_case.
        // We need to skip it. We can check if this child is the `value` field.
        let is_value = is_case
            && node
                .child_by_field_name("value")
                .is_some_and(|v| v.id() == child.id());
        if is_value {
            continue;
        }

        stmt_count += 1;
        if child.kind() == "statement_block" {
            has_block = true;
        }
    }

    // Fall-through case (no body) — skip
    if stmt_count == 0 {
        return;
    }

    // Already wrapped in a block — skip
    if stmt_count == 1 && has_block {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "switch-case-braces".into(),
        message: "Missing braces in `case` clause — wrap the body in `{ }` \
                  to avoid scope leaking."
            .into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_case_without_braces() {
        let src = r#"
switch (x) {
    case 'a':
        const y = 1;
        break;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "switch-case-braces");
    }

    #[test]
    fn allows_case_with_braces() {
        let src = r#"
switch (x) {
    case 'a': {
        const y = 1;
        break;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_fallthrough_case() {
        let src = r#"
switch (x) {
    case 'a':
    case 'b': {
        doSomething();
        break;
    }
}
"#;
        // case 'a' is a fall-through (no body), case 'b' has braces
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_default_without_braces() {
        let src = r#"
switch (x) {
    case 'a': {
        break;
    }
    default:
        const z = 2;
        break;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_multiple_cases_without_braces() {
        let src = r#"
switch (x) {
    case 'a':
        foo();
        break;
    case 'b':
        bar();
        break;
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }
}
