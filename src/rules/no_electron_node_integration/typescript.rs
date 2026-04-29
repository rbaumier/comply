//! no-electron-node-integration — flag `nodeIntegration*: true` inside
//! `webPreferences` of Electron `BrowserWindow` / `BrowserView` constructors.
//!
//! Enabling `nodeIntegration`, `nodeIntegrationInWorker`, or
//! `nodeIntegrationInSubFrames` gives renderer content direct access to
//! Node.js APIs, which is a well-known Electron security footgun.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

const BANNED_KEYS: &[&str] = &[
    "nodeIntegration",
    "nodeIntegrationInWorker",
    "nodeIntegrationInSubFrames",
];

fn unquote(s: &str) -> &str {
    s.trim_matches(|c: char| c == '\'' || c == '"' || c == '`')
}

/// Return the first `object` child of an expression node, skipping
/// parentheses / `as` casts so `new BrowserWindow({...} as Options)` still
/// matches.
fn unwrap_object(mut node: Node<'_>) -> Option<Node<'_>> {
    loop {
        match node.kind() {
            "object" => return Some(node),
            "parenthesized_expression" | "as_expression" | "satisfies_expression"
            | "type_assertion" => {
                node = node.named_child(0)?;
            }
            _ => return None,
        }
    }
}

/// Find a `pair` inside `object` whose key matches `name`.
fn find_pair<'a>(object: Node<'a>, source: &[u8], name: &str) -> Option<Node<'a>> {
    let mut cursor = object.walk();
    for child in object.named_children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let Some(key) = child.child_by_field_name("key") else {
            continue;
        };
        let key_text = unquote(key.utf8_text(source).unwrap_or("").trim());
        if key_text == name {
            return Some(child);
        }
    }
    None
}

crate::ast_check! { on ["new_expression"] prefilter = ["nodeIntegration"] => |node, source, ctx, diagnostics|
    let Some(constructor) = node.child_by_field_name("constructor") else { return };
    let ctor_name = constructor.utf8_text(source).unwrap_or("");
    // Match bare `BrowserWindow` or namespaced `electron.BrowserWindow`.
    let is_target = matches!(ctor_name, "BrowserWindow" | "BrowserView")
        || ctor_name.ends_with(".BrowserWindow")
        || ctor_name.ends_with(".BrowserView");
    if !is_target {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // `arguments` is a parenthesized list; find the first object-like argument.
    let mut cursor = args.walk();
    let options_object = args
        .named_children(&mut cursor)
        .find_map(unwrap_object);
    let Some(options_object) = options_object else { return };

    let Some(web_prefs_pair) = find_pair(options_object, source, "webPreferences") else { return };
    let Some(web_prefs_value) = web_prefs_pair.child_by_field_name("value") else { return };
    let Some(web_prefs_object) = unwrap_object(web_prefs_value) else { return };

    for key in BANNED_KEYS {
        let Some(pair) = find_pair(web_prefs_object, source, key) else { continue };
        let Some(value) = pair.child_by_field_name("value") else { continue };
        if value.utf8_text(source).unwrap_or("").trim() != "true" {
            continue;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &pair,
            super::META.id,
            format!(
                "`{key}: true` in Electron `webPreferences` exposes Node APIs to renderer content — remove it or set it to `false`."
            ),
            Severity::Error,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_node_integration_true_in_browser_window() {
        let src = "new BrowserWindow({ webPreferences: { nodeIntegration: true } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_node_integration_in_worker() {
        let src = "new BrowserWindow({ webPreferences: { nodeIntegrationInWorker: true } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_node_integration_in_sub_frames() {
        let src = "new BrowserView({ webPreferences: { nodeIntegrationInSubFrames: true } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiple_banned_flags_in_single_options() {
        let src = "new BrowserWindow({ webPreferences: { nodeIntegration: true, nodeIntegrationInWorker: true } });";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_namespaced_electron_browser_window() {
        let src = "new electron.BrowserWindow({ webPreferences: { nodeIntegration: true } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_node_integration_false() {
        let src = "new BrowserWindow({ webPreferences: { nodeIntegration: false } });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_browser_window_without_web_preferences() {
        let src = "new BrowserWindow({ width: 800, height: 600 });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_unrelated_constructors() {
        let src = "new OtherThing({ webPreferences: { nodeIntegration: true } });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_node_integration_outside_web_preferences() {
        let src = "new BrowserWindow({ nodeIntegration: true });";
        assert!(run(src).is_empty());
    }
}
