//! dockerfile-valid-port tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["expose_instruction"] => |node, source, ctx, diagnostics|
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() != "expose_port" { continue; }
        let Ok(text) = std::str::from_utf8(&source[child.byte_range()]) else { continue; };
        let port_str = text.split('/').next().unwrap_or(text).trim();
        if port_str.starts_with('$') { continue; }
        let valid = port_str.parse::<u32>().map(|n| n <= 65535).unwrap_or(false);
        if !valid {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: super::META.id.into(),
                message: format!("`{port_str}` is not a valid port number (0..=65535)."),
                severity: Severity::Warning,
                span: Some((child.byte_range().start, child.byte_range().len())),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_out_of_range_port() {
        assert_eq!(run("EXPOSE 80800\n").len(), 1);
    }

    #[test]
    fn allows_valid_port() {
        assert!(run("EXPOSE 8080\n").is_empty());
    }

    #[test]
    fn allows_valid_port_with_protocol() {
        assert!(run("EXPOSE 8080/tcp\n").is_empty());
    }
}
