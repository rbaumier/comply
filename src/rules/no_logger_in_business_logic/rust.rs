//! no-logger-in-business-logic Rust backend — flag logging macros in
//! service/domain/core/model/entity layers.
//!
//! Walks `macro_invocation` nodes whose macro name is one of
//! `println!`/`eprintln!`/`log::*!`/`tracing::*!`. Files outside a
//! business-logic directory are ignored.

use crate::diagnostic::{Diagnostic, Severity};

const BUSINESS_DIRS: &[&str] = &["service", "domain", "core", "model", "entity"];

const LOG_LEVELS: &[&str] = &["trace", "debug", "info", "warn", "error"];
const LOG_NAMESPACES: &[&str] = &["log", "tracing"];

fn is_business_logic_path(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    BUSINESS_DIRS.iter().any(|dir| {
        path_str.contains(&format!("/{dir}/")) || path_str.contains(&format!("\\{dir}\\"))
    })
}

/// True if `name` matches one of our targeted logging macro paths and
/// returns the canonical pattern label (e.g. `"log::info!"`, `"println!"`).
fn classify_macro(name: &str) -> Option<String> {
    if name == "println" || name == "eprintln" {
        return Some(format!("{name}!"));
    }
    let mut segments = name.split("::");
    let ns = segments.next()?;
    let level = segments.next()?;
    if segments.next().is_some() {
        return None;
    }
    if !LOG_NAMESPACES.contains(&ns) {
        return None;
    }
    if !LOG_LEVELS.contains(&level) {
        return None;
    }
    Some(format!("{ns}::{level}!"))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "macro_invocation" {
        return;
    }

    if !is_business_logic_path(ctx.path) {
        return;
    }

    let Some(macro_name_node) = node.child_by_field_name("macro") else { return };
    let Ok(name) = macro_name_node.utf8_text(source) else { return };

    let Some(pattern) = classify_macro(name) else { return };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-logger-in-business-logic".into(),
        message: format!(
            "`{pattern}` in business logic \u{2014} use a wrapper or domain events instead."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::AstCheck;

    fn run_path(path: &str, source: &str) -> Vec<Diagnostic> {
        use std::path::Path;
        let ctx = crate::rules::backend::CheckCtx::for_test(Path::new(path), source);
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("grammar should load");
        let tree = parser.parse(source, None).expect("parser should produce a tree");
        Check.check(&ctx, &tree)
    }

    #[test]
    fn flags_println_in_service() {
        let diags = run_path("src/service/user.rs", r#"println!("creating user");"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_log_info_in_domain() {
        let diags = run_path("src/domain/order.rs", r#"log::info!("order placed");"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_logging_outside_business_dirs() {
        let diags = run_path("src/api/handler.rs", r#"println!("request received");"#);
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_comments_mentioning_logger() {
        let diags = run_path("src/service/user.rs", "// log::info! was removed");
        assert!(diags.is_empty());
    }
}
