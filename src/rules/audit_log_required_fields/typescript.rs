//! audit-log-required-fields backend — flag calls to audit-logging helpers
//! (`auditLog`, `audit.log`, `logger.audit`, `audit`) whose payload object
//! is missing one of the required keys: `userId`, `timestamp`, `action`.

use crate::diagnostic::{Diagnostic, Severity};

const AUDIT_FN_NAMES: &[&str] = &["auditLog", "audit"];

/// Either a bare `audit.log(...)` / `logger.audit(...)` or a method named
/// `audit` / `auditLog` on any receiver.
fn is_audit_call(name: &str) -> bool {
    if AUDIT_FN_NAMES.contains(&name) {
        return true;
    }
    let Some((receiver, method)) = name.rsplit_once('.') else {
        return false;
    };
    if AUDIT_FN_NAMES.contains(&method) {
        return true;
    }
    method == "log" && receiver.ends_with("audit")
}

const REQUIRED_KEYS: &[&[&str]] = &[
    &["userId", "user_id", "actorId", "actor_id"],
    &["timestamp", "ts", "createdAt", "created_at", "at", "time"],
    &["action", "event", "type", "verb"],
];

fn object_has_any_of(node: tree_sitter::Node, source: &[u8], keys: &[&str]) -> bool {
    if node.kind() != "object" {
        return false;
    }
    let mut cursor = node.walk();
    for prop in node.named_children(&mut cursor) {
        if prop.kind() != "pair" && prop.kind() != "shorthand_property_identifier" {
            continue;
        }
        let key_text: Option<&str> = match prop.kind() {
            "pair" => prop
                .child_by_field_name("key")
                .and_then(|k| k.utf8_text(source).ok()),
            "shorthand_property_identifier" => prop.utf8_text(source).ok(),
            _ => None,
        };
        let Some(mut name) = key_text else { continue };
        name = name.trim_matches(|c| c == '"' || c == '\'' || c == '`');
        if keys.contains(&name) {
            return true;
        }
    }
    false
}

fn object_missing_required(node: tree_sitter::Node, source: &[u8]) -> Option<&'static str> {
    for group in REQUIRED_KEYS {
        if !object_has_any_of(node, source, group) {
            return Some(group[0]);
        }
    }
    None
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !is_audit_call(name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(first) = args.named_children(&mut cursor).next() else {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "audit-log-required-fields",
            "Audit log call is missing required fields (`userId`, `timestamp`, `action`).".into(),
            Severity::Warning,
        ));
        return;
    };
    if first.kind() != "object" {
        return;
    }
    if let Some(missing) = object_missing_required(first, source) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "audit-log-required-fields",
            format!("Audit log entry is missing required field `{missing}` (or equivalent)."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_missing_user_id() {
        assert_eq!(
            run_on("auditLog({ action: 'login', timestamp: Date.now() })").len(),
            1
        );
    }

    #[test]
    fn flags_missing_timestamp() {
        assert_eq!(
            run_on("auditLog({ userId: u.id, action: 'login' })").len(),
            1
        );
    }

    #[test]
    fn flags_missing_action() {
        assert_eq!(
            run_on("auditLog({ userId: u.id, timestamp: Date.now() })").len(),
            1
        );
    }

    #[test]
    fn flags_audit_log_method_call() {
        assert_eq!(
            run_on("audit.log({ userId: u.id, timestamp: Date.now() })").len(),
            1
        );
    }

    #[test]
    fn allows_complete_audit_entry() {
        assert!(
            run_on("auditLog({ userId: u.id, action: 'login', timestamp: Date.now() })").is_empty()
        );
    }

    #[test]
    fn accepts_actor_id_alias() {
        assert!(
            run_on("auditLog({ actorId: u.id, event: 'login', createdAt: Date.now() })").is_empty()
        );
    }

    #[test]
    fn ignores_unrelated_log_call() {
        assert!(run_on("logger.info('hello')").is_empty());
    }
}
