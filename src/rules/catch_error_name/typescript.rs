//! catch-error-name backend — flag catch parameters not named `error`.

use crate::diagnostic::{Diagnostic, Severity};

const EXPECTED: &str = "error";

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "catch_clause" {
        return;
    }

    // The catch clause's parameter is in a `catch_parameter` wrapper node
    // that may or may not exist (bare `catch {}` has no parameter).
    // tree-sitter-typescript grammar: catch_clause -> "catch" "(" catch_parameter ")" block
    // The `catch_parameter` wraps the actual identifier/pattern.
    // We look for the identifier directly via named children.

    // Try field-based access first (some grammars expose `parameter`).
    let param = node.child_by_field_name("parameter");
    let param = match param {
        Some(p) => p,
        None => return, // bare `catch {}` — nothing to check
    };

    // The parameter might be a destructuring pattern — only check simple identifiers.
    let ident = if param.kind() == "identifier" {
        param
    } else {
        // Could be wrapped in a node; look for identifier child.
        match find_identifier(param) {
            Some(id) => id,
            None => return,
        }
    };

    let name = match ident.utf8_text(source) {
        Ok(n) => n,
        Err(_) => return,
    };

    // Allow `_` for unused catch parameters.
    if name == "_" {
        return;
    }

    // Allow names that are or end with `error` / `Error`.
    if name == EXPECTED
        || name.ends_with(EXPECTED)
        || name.ends_with("Error")
    {
        return;
    }

    let pos = ident.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "catch-error-name".into(),
        message: format!(
            "The catch parameter `{name}` should be named `{EXPECTED}`."
        ),
        severity: Severity::Warning,
    });
}

/// Walk immediate children to find the first `identifier` node.
fn find_identifier(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let count = node.named_child_count();
    for i in 0..count {
        if let Some(child) = node.named_child(i)
            && child.kind() == "identifier" {
                return Some(child);
            }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_catch_e() {
        let d = run_on("try {} catch (e) { console.log(e); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`e`"));
        assert!(d[0].message.contains("`error`"));
    }

    #[test]
    fn flags_catch_err() {
        let d = run_on("try {} catch (err) { throw err; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`err`"));
    }

    #[test]
    fn flags_catch_ex() {
        let d = run_on("try {} catch (ex) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_catch_error() {
        assert!(run_on("try {} catch (error) { throw error; }").is_empty());
    }

    #[test]
    fn allows_catch_underscore() {
        assert!(run_on("try {} catch (_) {}").is_empty());
    }

    #[test]
    fn allows_name_ending_with_error() {
        assert!(run_on("try {} catch (networkError) {}").is_empty());
    }

    #[test]
    fn allows_bare_catch() {
        assert!(run_on("try {} catch {}").is_empty());
    }
}
