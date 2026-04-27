//! elysia-model-reference-by-string — when a file imports a schema-looking
//! identifier (ending in `Schema` or `Model`) from a relative module and a
//! route options object passes that identifier directly as `body:`/`response:`
//! /`query:`, recommend using a registered model string reference instead.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

const SCHEMA_KEYS: &[&str] = &["body", "response", "query", "params", "headers"];

/// Walk top-level imports and collect identifiers ending in `Schema` /
/// `Model` brought in from a relative path.
fn collect_imported_schemas(root: tree_sitter::Node<'_>, source: &[u8]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "import_statement" {
            continue;
        }
        let raw = child.utf8_text(source).unwrap_or("");
        // Only consider relative imports — third-party libraries are out of scope.
        if !(raw.contains("from './") || raw.contains("from \"./") || raw.contains("from '../") || raw.contains("from \"../")) {
            continue;
        }
        let Some(open) = raw.find('{') else { continue };
        let Some(close_rel) = raw[open..].find('}') else { continue };
        let body = &raw[open + 1..open + close_rel];
        for chunk in body.split(',') {
            let imported = chunk.trim().trim_start_matches("type ").trim();
            let head = imported.split_whitespace().next().unwrap_or("");
            if head.ends_with("Schema") || head.ends_with("Model") {
                names.insert(head.to_string());
            }
        }
    }
    names
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    let root = match node.parent().and_then(|_| {
        // Walk up to find the program root.
        let mut cur = node;
        while let Some(p) = cur.parent() {
            cur = p;
        }
        Some(cur)
    }) {
        Some(r) => r,
        None => return,
    };
    let imports = collect_imported_schemas(root, source);
    if imports.is_empty() {
        return;
    }
    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key.utf8_text(source).unwrap_or("");
    let key_name = key_text.trim_matches(|c| c == '"' || c == '\'' || c == '`');
    if !SCHEMA_KEYS.contains(&key_name) {
        return;
    }
    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "identifier" {
        return;
    }
    let val_text = value.utf8_text(source).unwrap_or("");
    if !imports.contains(val_text) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-model-reference-by-string".into(),
        message: format!("`{}: {}` references an imported schema directly — register it with `.model({{ ... }})` and pass a string key for cross-route reuse.", key_name, val_text),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_imported_schema_used_inline() {
        let src = "import { UserSchema } from './schema';\napp.post('/x', () => 1, { body: UserSchema });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_string_reference() {
        let src = "import { UserSchema } from './schema';\napp.post('/x', () => 1, { body: 'user' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_inline_typebox() {
        let src = "import { t } from 'elysia';\napp.post('/x', () => 1, { body: t.Object({}) });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "import { UserSchema } from './schema';\napp.post('/x', () => 1, { body: UserSchema });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
