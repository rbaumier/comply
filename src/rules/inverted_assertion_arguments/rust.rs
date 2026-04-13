//! inverted-assertion-arguments Rust backend.
//!
//! Flag `assert_eq!(literal, variable)` — expected value should be second.

use crate::diagnostic::{Diagnostic, Severity};

fn is_literal(kind: &str) -> bool {
    matches!(
        kind,
        "integer_literal"
            | "float_literal"
            | "string_literal"
            | "raw_string_literal"
            | "char_literal"
            | "boolean_literal"
    )
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "macro_invocation" {
        return;
    }

    let Some(mac) = node.child_by_field_name("macro") else { return };
    let Ok(mac_name) = mac.utf8_text(source) else { return };

    if mac_name != "assert_eq" && mac_name != "assert_ne" {
        return;
    }

    // Get the token tree.
    let mut tt = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "token_tree" {
            tt = Some(child);
            break;
        }
    }
    let Some(args) = tt else { return };

    // Find the first two meaningful children (the arguments before the comma).
    let mut arg_nodes = Vec::new();
    let mut cursor2 = args.walk();
    for child in args.children(&mut cursor2) {
        let k = child.kind();
        if k == "(" || k == ")" {
            continue;
        }
        if k == "," {
            if !arg_nodes.is_empty() {
                break; // stop after first arg
            }
            continue;
        }
        arg_nodes.push(child);
    }

    if arg_nodes.is_empty() {
        return;
    }

    let first = arg_nodes[0];

    // Collect second arg (after comma).
    let mut after_comma = false;
    let mut second_nodes = Vec::new();
    let mut cursor3 = args.walk();
    for child in args.children(&mut cursor3) {
        let k = child.kind();
        if k == "(" || k == ")" {
            continue;
        }
        if k == "," {
            if !after_comma {
                after_comma = true;
                continue;
            }
            break; // stop at second comma (format args)
        }
        if after_comma {
            second_nodes.push(child);
        }
    }

    if second_nodes.is_empty() {
        return;
    }
    let second = second_nodes[0];

    // Flag if first arg is a literal and second is an identifier (inverted).
    if is_literal(first.kind()) && second.kind() == "identifier" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "inverted-assertion-arguments".into(),
            message: "Assertion arguments appear inverted — put the expected value second.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_literal_first() {
        assert_eq!(run_on("fn f() { assert_eq!(42, result); }").len(), 1);
    }

    #[test]
    fn allows_variable_first() {
        assert!(run_on("fn f() { assert_eq!(result, 42); }").is_empty());
    }
}
