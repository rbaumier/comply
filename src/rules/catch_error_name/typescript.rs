//! catch-error-name TypeScript / JavaScript / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["catch_clause"] => |node, source, ctx, diagnostics|
    // The catch parameter lives in the `parameter` field. A bare
    // `catch {}` with no parens exposes no parameter node, which is
    // fine — nothing to check.
    let Some(param) = node.child_by_field_name("parameter") else { return };

    // Destructuring patterns (`catch ({ message })`, `catch ([e])`)
    // aren't simple identifiers — the rule only applies to the
    // simple case. We skip anything that isn't a direct identifier.
    let ident = if param.kind() == "identifier" {
        param
    } else {
        match find_identifier(param) {
            Some(id) => id,
            None => return,
        }
    };

    let Ok(name) = ident.utf8_text(source) else { return };

    if super::is_acceptable_name(name) {
        return;
    }

    let pos = ident.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "catch-error-name".into(),
        message: format!(
            "The catch parameter `{name}` should be named `{}`.",
            super::EXPECTED
        ),
        severity: Severity::Warning,
        span: None,
    });
}

/// Scan direct named children of `node` for the first `identifier`.
/// Used when the parameter is wrapped in a grammar-specific node
/// (for example, `catch_parameter` on some tree-sitter versions).
fn find_identifier(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let count = node.named_child_count();
    for i in 0..count {
        if let Some(child) = node.named_child(i)
            && child.kind() == "identifier"
        {
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
    fn flags_catch_exception() {
        let d = run_on("try {} catch (exception) {}");
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
    fn allows_name_ending_with_error_lower() {
        assert!(run_on("try {} catch (networkerror) {}").is_empty());
    }

    #[test]
    fn allows_name_ending_with_error_camel() {
        assert!(run_on("try {} catch (networkError) {}").is_empty());
    }

    #[test]
    fn allows_inner_error_for_nested_catches() {
        let src = "try { try { a(); } catch (innerError) { b(); } } catch (error) {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_catch() {
        assert!(run_on("try {} catch {}").is_empty());
    }

    #[test]
    fn ignores_destructured_catch() {
        assert!(run_on("try {} catch ({ message }) {}").is_empty());
    }
}
