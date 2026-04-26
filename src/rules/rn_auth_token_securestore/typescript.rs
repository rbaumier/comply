//! Flags `AsyncStorage.setItem(...)` / `AsyncStorage.getItem(...)` calls whose
//! first arg is a string literal containing `token` or `auth`.

use crate::diagnostic::{Diagnostic, Severity};

fn key_is_sensitive(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("token") || lower.contains("auth")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }
    let Some(obj) = func.child_by_field_name("object") else { return };
    let Ok(obj_text) = obj.utf8_text(source) else { return };
    if obj_text != "AsyncStorage" { return; }
    let Some(prop) = func.child_by_field_name("property") else { return };
    let Ok(prop_name) = prop.utf8_text(source) else { return };
    if prop_name != "setItem" && prop_name != "getItem" { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first) = args.named_child(0) else { return };
    if first.kind() != "string" { return; }
    let Ok(raw) = first.utf8_text(source) else { return };
    let key = raw.trim_matches(|c| c == '"' || c == '\'');
    if !key_is_sensitive(key) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("AsyncStorage is unencrypted — store `{key}` in expo-secure-store instead."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_auth_token_set() {
        let src = "AsyncStorage.setItem('auth_token', v);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_authtoken_get() {
        let src = "AsyncStorage.getItem('authToken');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_non_sensitive_key() {
        let src = "AsyncStorage.setItem('lastScreen', v);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_securestore() {
        let src = "SecureStore.setItemAsync('auth_token', v);";
        assert!(run(src).is_empty());
    }
}
