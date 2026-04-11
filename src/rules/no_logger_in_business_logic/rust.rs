//! no-logger-in-business-logic Rust backend — flag logging macros in
//! service/domain/core/model/entity layers.

use crate::diagnostic::{Diagnostic, Severity};

const BUSINESS_DIRS: &[&str] = &["service", "domain", "core", "model", "entity"];

const LOG_PATTERNS: &[&str] = &[
    "log::trace!",
    "log::debug!",
    "log::info!",
    "log::warn!",
    "log::error!",
    "tracing::trace!",
    "tracing::debug!",
    "tracing::info!",
    "tracing::warn!",
    "tracing::error!",
    "println!",
    "eprintln!",
];

fn is_business_logic_path(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    BUSINESS_DIRS.iter().any(|dir| {
        path_str.contains(&format!("/{dir}/")) || path_str.contains(&format!("\\{dir}\\"))
    })
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "source_file" {
        return;
    }

    if !is_business_logic_path(ctx.path) {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        // Skip comments.
        if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
            continue;
        }
        for pattern in LOG_PATTERNS {
            if trimmed.contains(pattern) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-logger-in-business-logic".into(),
                    message: format!(
                        "`{pattern}` in business logic \u{2014} use a wrapper or domain events instead."
                    ),
                    severity: Severity::Warning,
                });
                break;
            }
        }
    }
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
