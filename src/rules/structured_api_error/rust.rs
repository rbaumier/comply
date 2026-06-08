//! structured-api-error Rust backend.
//!
//! Flags `panic!` in HTTP routing files.
//! In Rust, route handlers typically use Actix-web, Axum, or Rocket.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_in_test_context, is_under_tests_dir};

fn is_route_file(source: &[u8]) -> bool {
    let src = std::str::from_utf8(source).unwrap_or("");
    src.lines().any(|line| {
        let t = line.trim();
        t.contains("#[get(")
            || t.contains("#[post(")
            || t.contains("#[put(")
            || t.contains("#[delete(")
            || t.contains("#[patch(")
            || t.contains(".route(")
            || t.contains("Router::new()")
    })
}

crate::ast_check! { on ["macro_invocation"] => |node, source, ctx, diagnostics|
    let Some(mac) = node.child_by_field_name("macro") else { return };
    let Ok(mac_name) = mac.utf8_text(source) else { return };

    if is_in_test_context(node, source) || is_under_tests_dir(ctx.path) {
        return;
    }

    if mac_name != "panic" {
        return;
    }

    if !is_route_file(source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "structured-api-error".into(),
        message: "Bare `panic!` in route handler — use structured error types.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_panic_in_route() {
        let src = "fn setup() { let _ = Router::new(); panic!(\"oops\"); }\n";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_panic_outside_route() {
        let src = "fn handler() { panic!(\"oops\"); }\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_panic_in_test_dir() {
        // Router::new() would trigger is_route_file, but path is under tests/
        let src = "use axum::Router;\nfn setup() { let _ = Router::new(); panic!(\"oops\"); }\n";
        assert!(crate::rules::test_helpers::run_rust_with_path(src, &Check, "tests/helper.rs").is_empty());
    }

    #[test]
    fn ignores_panic_in_cfg_test_module() {
        let src = "#[cfg(test)]\nmod tests {\n    use axum::Router;\n    fn helper() { let _ = Router::new(); panic!(\"oops\"); }\n}\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_panic_in_utility_module() {
        // Reproduces FP from axum/src/response/sse.rs: file imports axum but is not a handler/router
        let src = r#"
use axum::response::sse::Sse;

pub struct Event { flags: u8 }

impl Event {
    pub fn event(mut self) -> Self {
        if self.flags & 1 != 0 {
            panic!("Called Event::event multiple times");
        }
        self
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
