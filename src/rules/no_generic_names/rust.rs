//! no-generic-names backend for Rust.
//!
//! Flags a binding, parameter, function, const, or static whose *whole* name
//! is a generic placeholder — the shared `super::GENERIC_WORDS` core plus the
//! Rust-specific extras below. Whole-name match only: `rows`/`result`/`data`
//! flag, but a descriptive compound (`affected_rows`, `query_result`,
//! `user_data`) does not. The TypeScript backend's segment (`data` inside
//! `getUserData`) and prefix-verb (`processOrder`) matching are TS-only; Rust
//! keeps the simpler exact-name rule.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

/// Rust-specific additions to `super::GENERIC_WORDS`. `data` is generic as a
/// bare Rust binding, but the TypeScript backend already catches it through its
/// segment matcher (`userData`), so it lives here rather than in the shared
/// core. Rust idioms stay off this list on purpose: `ptr` (FFI), `body` (HTTP
/// handlers), `entry`/`entries` (map APIs), `vec`/`str`/`num` (type-named
/// locals) read as generic in TS but are conventional in Rust.
pub(super) const EXTRA_BANNED_WORDS: &[&str] = &["data"];

/// Words allowed as a *parameter* name even though they are banned as a
/// binding. `fn from(value: T)` / `impl Iterator … (item)` are the idiomatic,
/// trait-prescribed parameter names — the caller has no rename freedom — so a
/// generic name there is conventional, not lazy. Mirrors the TypeScript
/// backend's `PARAM_ALLOWED_WORDS`.
const PARAM_ALLOWED_WORDS: &[&str] = &["value", "item"];

/// Parent node kinds whose child `identifier` is a binding site the rule owns.
/// Matches `no_abbreviated_names`: `let`, function/closure parameters, function
/// names, and module-level constants. Struct fields (`field_identifier`) and
/// type names (`type_identifier`) are a different node kind and out of scope.
const BINDING_PARENTS: &[&str] = &[
    "let_declaration",
    "parameter",
    "function_item",
    "const_item",
    "static_item",
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
        let Some(parent) = node.parent() else {
            return;
        };
        if !BINDING_PARENTS.contains(&parent.kind()) {
            return;
        }
        let Ok(name) = node.utf8_text(ctx.source.as_bytes()) else {
            return;
        };
        let lower = name.to_ascii_lowercase();

        let allowed = ctx.config.string_list("no-generic-names", "allowed", ctx.lang);
        if allowed.iter().any(|a| a.eq_ignore_ascii_case(&lower)) {
            return;
        }
        // A trait-prescribed parameter name (`from(value)`, `next() -> item`) is
        // conventional, not lazy.
        if parent.kind() == "parameter" && PARAM_ALLOWED_WORDS.contains(&lower.as_str()) {
            return;
        }
        if !is_generic_word(&lower, ctx) {
            return;
        }

        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: format!(
                "Identifier '{name}' carries no meaning — rename to describe \
                 what the value IS (`parsed_order`, `user_profile`, \
                 `payment_receipt`)."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when `lower` (already lowercased) is a banned generic word — the shared
/// core, the Rust extras, or a project-configured extra from `comply.toml`.
fn is_generic_word(lower: &str, ctx: &CheckCtx) -> bool {
    if super::GENERIC_WORDS.contains(&lower) || EXTRA_BANNED_WORDS.contains(&lower) {
        return true;
    }
    ctx.config
        .string_list("no-generic-names", "banned", ctx.lang)
        .iter()
        .any(|b| b.eq_ignore_ascii_case(lower))
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_bare_rows_binding() {
        // The natalia-v2 case: `let rows = sqlx::query!(...).fetch_all(...)`.
        let diags = run_on("fn f() { let rows = fetch(); }");
        assert!(diags.iter().any(|d| d.message.contains("rows")));
    }

    #[test]
    fn flags_common_generic_bindings() {
        for name in ["result", "tmp", "temp", "val", "value", "obj", "foo", "data"] {
            let src = format!("fn f() {{ let {name} = compute(); }}");
            assert!(
                !run_on(&src).is_empty(),
                "'{name}' must be flagged as a generic binding"
            );
        }
    }

    #[test]
    fn allows_descriptive_compounds() {
        // Whole-name match only — a descriptive compound is not generic.
        assert!(run_on("fn f() { let affected_rows = purge(); }").is_empty());
        assert!(run_on("fn f() { let query_result = run(); }").is_empty());
        assert!(run_on("fn f() { let user_data = load(); }").is_empty());
        assert!(run_on("fn f() { let row_count = count(); }").is_empty());
    }

    #[test]
    fn allows_trait_prescribed_parameter_names() {
        // `From::from(value)` and iterator `item` are conventional parameter
        // names, not lazy placeholders.
        assert!(run_on("fn from(value: Wire) -> Self { Self(value) }").is_empty());
        assert!(run_on("fn push(item: T) {}").is_empty());
    }

    #[test]
    fn still_flags_generic_names_as_bindings_not_params() {
        // The parameter allowlist does not extend to `let` bindings.
        assert!(!run_on("fn f() { let value = compute(); }").is_empty());
        assert!(!run_on("fn f() { let item = next(); }").is_empty());
    }

    #[test]
    fn flags_generic_function_name() {
        assert!(!run_on("fn foo() {}").is_empty());
    }

    #[test]
    fn ignores_struct_fields_and_usages() {
        // Field names are a different node kind (out of scope), and a banned
        // word used as an expression (method receiver) is not a binding.
        assert!(run_on("struct S { data: i32 }").is_empty());
        assert!(run_on("fn f(conn: Conn) { let count = conn.rows(); }").is_empty());
    }
}
