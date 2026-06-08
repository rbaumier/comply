//! no-incorrect-string-concat AST backend — flag `"..." + identifier`
//! where the identifier's name suggests it holds a number.
//!
//! Walks `binary_expression` nodes with operator `+` whose left side is a
//! string literal and whose right side is an identifier (or member
//! expression) whose final segment name contains a known "numeric hint"
//! (`count`, `total`, `index`, …). Symmetrically flags `identifier + "..."`.

use crate::diagnostic::{Diagnostic, Severity};

const NUMERIC_HINTS: &[&str] = &[
    "count", "num", "total", "index", "length", "size", "amount", "qty", "sum", "age", "port",
    "offset", "width", "height", "price", "cost",
];

/// Extract a final identifier name from a node that's an identifier or a
/// dotted member expression (e.g. `obj.prop` → `prop`).
fn final_ident_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier" | "property_identifier" => node.utf8_text(source).ok(),
        "member_expression" => {
            let prop = node.child_by_field_name("property")?;
            prop.utf8_text(source).ok()
        }
        _ => None,
    }
}

/// True if `name` looks like a numeric value based on the hint list.
fn looks_numeric(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    NUMERIC_HINTS.iter().any(|h| lower.contains(h))
}

/// True if `node` is a string literal (TS `string`) or a template string.
fn is_string_literal(node: tree_sitter::Node) -> bool {
    matches!(node.kind(), "string")
}

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let op = node
        .child_by_field_name("operator")
        .and_then(|n| n.utf8_text(source).ok());
    if op != Some("+") {
        return;
    }
    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    // Pattern A: "string" + numericIdent
    let flagged = if is_string_literal(left)
        && let Some(name) = final_ident_name(right, source)
        && looks_numeric(name)
    {
        true
    // Pattern B: numericIdent + "string"
    } else if is_string_literal(right)
        && let Some(name) = final_ident_name(left, source)
        && looks_numeric(name)
    {
        true
    } else {
        false
    };

    if flagged {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "no-incorrect-string-concat",
            "Suspicious string concatenation with a numeric variable \u{2014} use explicit conversion or template literals.".into(),
            Severity::Warning,
        ));
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_string_plus_count() {
        assert_eq!(run_on(r#"const msg = "Total: " + itemCount;"#).len(), 1);
    }

    #[test]
    fn flags_string_plus_total() {
        assert_eq!(run_on(r#"console.log("Sum is " + totalAmount);"#).len(), 1);
    }

    #[test]
    fn allows_string_plus_string_var() {
        assert!(run_on(r#"const msg = "Hello " + userName;"#).is_empty());
    }

    #[test]
    fn allows_template_literal() {
        assert!(run_on(r#"const msg = `Total: ${itemCount}`;"#).is_empty());
    }
}
