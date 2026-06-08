//! elysia-t-unknown-format-string — flag `t.String({ format: '...' })` whose
//! `format` value is not in the well-known JSON-schema format whitelist.

use crate::diagnostic::{Diagnostic, Severity};

const KNOWN_FORMATS: &[&str] = &[
    "email",
    "uri",
    "uuid",
    "date",
    "date-time",
    "ipv4",
    "ipv6",
    "hostname",
    "regex",
    "time",
    "duration",
    "json-pointer",
    "relative-json-pointer",
    "uri-reference",
    "uri-template",
    "iri",
    "iri-reference",
    "idn-email",
    "idn-hostname",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "t.String" {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    // Look for an object literal child with `format: 'value'`.
    let mut cursor = args.walk();
    for arg in args.named_children(&mut cursor) {
        if arg.kind() != "object" {
            continue;
        }
        let mut acursor = arg.walk();
        for pair in arg.named_children(&mut acursor) {
            if pair.kind() != "pair" {
                continue;
            }
            let Some(key) = pair.child_by_field_name("key") else { continue };
            let key_text = key.utf8_text(source).unwrap_or("");
            let key_name = key_text.trim_matches(|c| c == '"' || c == '\'' || c == '`');
            if key_name != "format" {
                continue;
            }
            let Some(value) = pair.child_by_field_name("value") else { continue };
            if value.kind() != "string" {
                continue;
            }
            let raw = value.utf8_text(source).unwrap_or("");
            let format_str = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`');
            if KNOWN_FORMATS.contains(&format_str) {
                continue;
            }
            let pos = pair.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "elysia-t-unknown-format-string".into(),
                message: format!("`format: '{}'` is not a recognised JSON-schema format — TypeBox will silently skip the check.", format_str),
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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_unknown_format() {
        let src = "import { t } from 'elysia';\nconst s = t.String({ format: 'emial' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_known_format_email() {
        let src = "import { t } from 'elysia';\nconst s = t.String({ format: 'email' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_known_format_date_time() {
        let src = "import { t } from 'elysia';\nconst s = t.String({ format: 'date-time' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_string_without_format() {
        let src = "import { t } from 'elysia';\nconst s = t.String({ minLength: 1 });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "const s = t.String({ format: 'emial' });";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
