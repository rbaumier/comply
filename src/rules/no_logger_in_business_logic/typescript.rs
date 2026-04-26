//! no-logger-in-business-logic AST backend — flag logging calls in
//! service/domain/core/model/entity layers.
//!
//! Walks `call_expression` nodes whose callee is a `member_expression`
//! whose object is `console` (with property `log`/`info`/`warn`/`error`/
//! `debug`/`trace`) or whose root object is `logger`. Files outside a
//! business-logic directory are ignored.

use crate::diagnostic::{Diagnostic, Severity};

const BUSINESS_DIRS: &[&str] = &["service", "domain", "core", "model", "entity"];

const CONSOLE_METHODS: &[&str] = &["log", "info", "warn", "error", "debug", "trace"];

fn is_business_logic_path(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    BUSINESS_DIRS.iter().any(|dir| {
        path_str.contains(&format!("/{dir}/")) || path_str.contains(&format!("\\{dir}\\"))
    })
}

/// Return the leftmost identifier in a (possibly chained) member expression.
/// `console.log` -> `console`; `logger.scoped.info` -> `logger`.
fn root_identifier<'a>(mut node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    loop {
        match node.kind() {
            "identifier" | "this" => {
                return node.utf8_text(source).ok();
            }
            "member_expression" => {
                node = node.child_by_field_name("object")?;
            }
            _ => return None,
        }
    }
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_business_logic_path(ctx.path) {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(object) = callee.child_by_field_name("object") else { return };
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let Ok(prop_text) = prop.utf8_text(source) else { return };

    let Some(root) = root_identifier(object, source) else { return };

    let pattern = match root {
        "console" if CONSOLE_METHODS.contains(&prop_text) => format!("console.{prop_text}"),
        "logger" => "logger.".to_string(),
        _ => return,
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-logger-in-business-logic".into(),
        message: format!(
            "`{pattern}` in business logic — use a `withLogging()` wrapper or domain events instead."
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
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("grammar should load");
        let tree = parser.parse(source, None).expect("parser should produce a tree");
        Check.check(&ctx, &tree)
    }

    #[test]
    fn flags_console_log_in_service() {
        let diags = run_path("src/service/user.ts", "console.log('creating user');");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_logger_in_domain() {
        let diags = run_path("src/domain/order.ts", "logger.info('order placed');");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_logging_outside_business_dirs() {
        let diags = run_path("src/api/handler.ts", "console.log('request received');");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_comments_mentioning_logger() {
        let diags = run_path("src/service/user.ts", "// logger.info was removed");
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_console_warn_in_core() {
        let diags = run_path("src/core/pricing.ts", "console.warn('price is zero');");
        assert_eq!(diags.len(), 1);
    }
}
