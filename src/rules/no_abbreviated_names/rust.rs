//! no-abbreviated-names backend for Rust.
//!
//! Same dictionary as the TypeScript impl, applied to Rust identifiers.
//! Splits snake_case words (Rust convention) and checks each against a
//! banned abbreviation list.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

// Match the TypeScript list: only flag GENUINELY obscure abbreviations,
// not ecosystem idioms. `cfg`, `ctx`, `idx`, `err`, `fmt`, `ret`, `val`,
// `num`, `str`, `obj`, `arr`, `req`, `res`, `msg`, `auth`, `db`, `dict`
// are all part of the Rust vocabulary (cfg attributes, std::fmt, io
// context, iteration index, …) and flagging them only adds noise. The
// list targets abbreviations a reader genuinely has to guess about.
const BANNED_ABBREVIATIONS: &[(&str, &str)] = &[
    ("acct", "account"),
    ("usr", "user"),
    ("btn", "button"),
    ("pwd", "password"),
    ("cnt", "count"),
    ("desc", "description"),
    // `addr` is intentionally NOT on the list — `std::net::SocketAddr`,
    // `peer_addr()`, `local_addr()`, `bind_addr` are all standard Rust API.
    // `tmp` is intentionally NOT on the list — `std::env::temp_dir()`
    // and `tempfile::NamedTempFile::new()?.path()` are idiomatic Rust
    // and the `tmp` binding name follows std convention.
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["identifier"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        // Only flag at declaration sites.
        let Some(parent) = node.parent() else {
            return;
        };
        if !matches!(
            parent.kind(),
            "let_declaration" | "parameter" | "function_item" | "const_item" | "static_item"
        ) {
            return;
        }
        let Ok(name) = node.utf8_text(source_bytes) else {
            return;
        };
        let Some((abbr, full)) = matches_banned(name) else {
            return;
        };
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-abbreviated-names".into(),
            message: format!(
                "Identifier '{name}' contains abbreviation '{abbr}' — \
                 use the full word '{full}'. Editors auto-complete; \
                 readers don't."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn matches_banned(name: &str) -> Option<(&'static str, &'static str)> {
    for word in name.split('_') {
        let lower = word.to_ascii_lowercase();
        if let Some(&pair) = BANNED_ABBREVIATIONS.iter().find(|(abbr, _)| lower == *abbr) {
            return Some(pair);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_snake_case_abbreviation() {
        let diags = run_on("fn f() { let user_acct = 1; }");
        assert!(diags.iter().any(|d| d.message.contains("acct")));
    }

    #[test]
    fn flags_bare_abbreviation() {
        let diags = run_on("fn f() { let btn = 1; }");
        assert!(diags.iter().any(|d| d.message.contains("btn")));
    }

    #[test]
    fn allows_full_words() {
        assert!(run_on("fn f() { let user_account = 1; }").is_empty());
        assert!(run_on("fn f() { let request_context = 1; }").is_empty());
    }

    #[test]
    fn allows_rust_ecosystem_idioms() {
        // cfg, ctx, idx, err, fmt, ret, val, num, str, obj, arr, req,
        // res, msg, auth, db, dict — all part of the Rust vocabulary
        // and intentionally NOT flagged.
        assert!(run_on("fn f(ctx: &Context) {}").is_empty());
        assert!(run_on("fn f(idx: usize) {}").is_empty());
        assert!(run_on("fn f() { let cfg = 1; }").is_empty());
        assert!(run_on("fn f(err: Error) {}").is_empty());
        assert!(run_on("fn f() { let fmt = 1; }").is_empty());
    }

    #[test]
    fn flags_param_abbreviation() {
        let diags = run_on("fn f(usr_id: usize) {}");
        assert!(diags.iter().any(|d| d.message.contains("usr")));
    }

    #[test]
    fn does_not_flag_word_containing_abbreviation_letters() {
        // 'account' contains 'acct' letters but isn't the abbreviation.
        assert!(run_on("fn f() { let accountant = 1; }").is_empty());
    }
}
