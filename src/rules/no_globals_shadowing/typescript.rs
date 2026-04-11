//! no-globals-shadowing backend — flag local variables that shadow
//! well-known globals.

use crate::diagnostic::{Diagnostic, Severity};

const SHADOWED_GLOBALS: &[&str] = &[
    "console",
    "window",
    "document",
    "process",
    "global",
    "globalThis",
    "setTimeout",
    "setInterval",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "variable_declarator" {
        return;
    }
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };
    if !SHADOWED_GLOBALS.contains(&name) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-globals-shadowing".into(),
        message: format!(
            "Local variable shadows global `{name}` — rename to avoid confusion."
        ),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_const_console() {
        assert_eq!(run_on("const console = {};").len(), 1);
    }

    #[test]
    fn flags_let_window() {
        assert_eq!(run_on("let window = {};").len(), 1);
    }

    #[test]
    fn allows_different_name() {
        assert!(run_on("const myConsole = {};").is_empty());
    }

    #[test]
    fn allows_console_usage() {
        assert!(run_on("console.log('hello');").is_empty());
    }
}
