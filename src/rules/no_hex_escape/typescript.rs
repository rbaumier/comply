//! no-hex-escape backend — flag `\xNN` hex escapes, prefer `\u00NN`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["string", "template_string", "string_fragment"] => |node, source, ctx, diagnostics|
    // Only check string/template literals.
match node.kind() {
        "string" | "template_string" | "string_fragment" => {}
        _ => return,
    }

    let Ok(text) = node.utf8_text(source) else { return };

    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 3 < len {
        if bytes[i] == b'\\' {
            let bs_start = i;
            while i < len && bytes[i] == b'\\' {
                i += 1;
            }
            let bs_count = i - bs_start;

            if bs_count % 2 == 1
                && i < len
                && bytes[i] == b'x'
                && i + 2 < len
                && bytes[i + 1].is_ascii_hexdigit()
                && bytes[i + 2].is_ascii_hexdigit()
            {
                let hex = &text[i + 1..i + 3];
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-hex-escape".into(),
                    message: format!(
                        "Use Unicode escape `\\u00{}` instead of hex escape `\\x{}`.",
                        hex, hex
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                i += 3;
            }
        } else {
            i += 1;
        }
    }
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_hex_escape_in_string() {
        let d = run_on(r#"const x = '\x41';"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("\\u0041"));
    }

    #[test]
    fn allows_unicode_escape() {
        assert!(run_on(r#"const x = '\u0041';"#).is_empty());
    }

    #[test]
    fn allows_escaped_backslash_before_x() {
        assert!(run_on(r#"const x = '\\x41';"#).is_empty());
    }

    #[test]
    fn allows_normal_string() {
        assert!(run_on(r#"const x = "hello";"#).is_empty());
    }
}
