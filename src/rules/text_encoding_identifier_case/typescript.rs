//! text-encoding-identifier-case backend — walk `string` / `template_string`
//! nodes and flag encoding identifiers with wrong casing.
//!
//! Detection: check string literal nodes for encoding identifiers like
//! "UTF-8", "ASCII" that should be lowercase.

use crate::diagnostic::{Diagnostic, Severity};

/// Known encoding identifiers and their canonical lowercase form.
const ENCODINGS: &[(&str, &str)] = &[
    ("UTF-8", "utf-8"),
    ("Utf-8", "utf-8"),
    ("UTF8", "utf8"),
    ("Utf8", "utf8"),
    ("ASCII", "ascii"),
    ("Ascii", "ascii"),
];

crate::ast_check! { on ["string_fragment"] => |node, source, ctx, diagnostics|
    // Only check string_fragment to avoid double-counting (parent `string` also matches).
    let text = &source[node.byte_range()];
    let Ok(content) = std::str::from_utf8(text) else {
        return;
    };

    for &(bad, good) in ENCODINGS {
        if content == bad {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "text-encoding-identifier-case".into(),
                message: format!("Prefer `'{good}'` over `'{bad}'`."),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_uppercase_utf8_dash() {
        let d = run_on(r#"const enc = "UTF-8";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("utf-8"));
    }

    #[test]
    fn flags_mixed_case_utf8() {
        let d = run_on(r#"const enc = 'Utf-8';"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("utf-8"));
    }

    #[test]
    fn flags_uppercase_ascii() {
        let d = run_on(r#"const enc = "ASCII";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("ascii"));
    }

    #[test]
    fn allows_lowercase_utf8() {
        assert!(run_on(r#"const enc = "utf-8";"#).is_empty());
    }

    #[test]
    fn allows_lowercase_ascii() {
        assert!(run_on(r#"const enc = 'ascii';"#).is_empty());
    }
}
