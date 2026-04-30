//! Detects client-side navigation through `window.location` and `location.*`.
//!
//! Flags:
//! - `window.location = '/x'`
//! - `window.location.href = '/x'` (or `location.href = '/x'`)
//! - `window.location.replace('/x')` / `.assign('/x')`
//! - `location.replace('/x')` / `location.assign('/x')`
//!
//! These bypass Next.js routing and force a full document reload.

use crate::diagnostic::{Diagnostic, Severity};

/// True when `node` is `window` or `location` identifier.
fn is_window_or_location(node: tree_sitter::Node, source: &[u8]) -> Option<&'static str> {
    if node.kind() != "identifier" {
        return None;
    }
    match node.utf8_text(source).ok()? {
        "window" => Some("window"),
        "location" => Some("location"),
        _ => None,
    }
}

/// Returns the property name (e.g. `location`, `href`) of a member expression.
fn member_property_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name("property")?.utf8_text(source).ok()
}

/// True if `node` is `window.location` or `location` (bare).
fn is_window_location_target(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "member_expression" {
        let Some(obj) = node.child_by_field_name("object") else {
            return false;
        };
        if is_window_or_location(obj, source) == Some("window")
            && member_property_name(node, source) == Some("location")
        {
            return true;
        }
    }
    is_window_or_location(node, source) == Some("location")
}

fn report(
    node: tree_sitter::Node,
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
    msg: &str,
) {
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: msg.into(),
        severity: Severity::Warning,
        span: None,
    });
}

crate::ast_check! { on ["assignment_expression", "call_expression"] => |node, source, ctx, diagnostics|
    match node.kind() {
        "assignment_expression" => {
            let Some(left) = node.child_by_field_name("left") else { return };
            if left.kind() != "member_expression" {
                return;
            }
            let Some(obj) = left.child_by_field_name("object") else { return };
            let Some(prop) = member_property_name(left, source) else { return };

            // window.location = '/x'
            if is_window_or_location(obj, source) == Some("window") && prop == "location" {
                report(node, ctx, diagnostics,
                    "Assigning to `window.location` triggers a full page reload — use Next.js `redirect()` or `useRouter().push()`.");
                return;
            }

            // window.location.href = '/x'  OR  location.href = '/x'
            if prop == "href" && is_window_location_target(obj, source) {
                report(node, ctx, diagnostics,
                    "Assigning to `location.href` triggers a full page reload — use Next.js `redirect()` or `useRouter().push()`.");
            }
        }
        "call_expression" => {
            let Some(callee) = node.child_by_field_name("function") else { return };
            if callee.kind() != "member_expression" {
                return;
            }
            let Some(method) = member_property_name(callee, source) else { return };
            if method != "replace" && method != "assign" {
                return;
            }
            let Some(obj) = callee.child_by_field_name("object") else { return };
            if !is_window_location_target(obj, source) {
                return;
            }
            report(node, ctx, diagnostics,
                &format!("`location.{method}()` triggers a full page reload — use Next.js `redirect()` or `useRouter().push()`."));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_window_location_assignment() {
        let diags = run("function f() { window.location = '/home'; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("window.location"));
    }

    #[test]
    fn flags_window_location_href_assignment() {
        let diags = run("function f() { window.location.href = '/home'; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("location.href"));
    }

    #[test]
    fn flags_location_href_assignment() {
        let diags = run("function f() { location.href = '/home'; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_window_location_replace_call() {
        let diags = run("function f() { window.location.replace('/home'); }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("location.replace"));
    }

    #[test]
    fn flags_location_assign_call() {
        let diags = run("function f() { location.assign('/home'); }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("location.assign"));
    }

    #[test]
    fn allows_router_push() {
        assert!(run("function f(router) { router.push('/home'); }").is_empty());
    }

    #[test]
    fn allows_redirect_call() {
        assert!(run("function f() { redirect('/home'); }").is_empty());
    }

    #[test]
    fn allows_unrelated_replace() {
        assert!(run("function f(s) { return s.replace('a', 'b'); }").is_empty());
    }
}
