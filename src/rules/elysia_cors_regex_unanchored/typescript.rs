//! elysia-cors-regex-unanchored backend — flag CORS regex origin missing trailing `$`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "cors" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");

    // Find `origin:` followed by a regex literal `/.../` and check it ends with `$/`.
    let mut idx = 0;
    let bytes = args_text.as_bytes();
    while idx < bytes.len() {
        if let Some(rest) = args_text.get(idx..) {
            if let Some(off) = rest.find("origin:") {
                let after = &rest[off + "origin:".len()..];
                // Skip whitespace.
                let after_trim = after.trim_start();
                if after_trim.starts_with('/') {
                    // Find the closing `/` (skip escaped).
                    let body = &after_trim[1..];
                    let mut end = None;
                    let mut esc = false;
                    for (i, c) in body.char_indices() {
                        if esc { esc = false; continue; }
                        if c == '\\' { esc = true; continue; }
                        if c == '/' { end = Some(i); break; }
                    }
                    if let Some(e) = end {
                        let regex_body = &body[..e];
                        if !regex_body.ends_with('$') {
                            let pos = node.start_position();
                            diagnostics.push(Diagnostic {
                                path: std::sync::Arc::clone(&ctx.path_arc),
                                line: pos.row + 1,
                                column: pos.column + 1,
                                rule_id: "elysia-cors-regex-unanchored".into(),
                                message: "CORS origin regex is not anchored with `$` — may match unintended origins.".into(),
                                severity: Severity::Warning,
                                span: None,
                            });
                        }
                    }
                }
                idx += off + "origin:".len();
                continue;
            }
        }
        break;
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
    fn flags_unanchored_regex() {
        let src =
            "import { cors } from '@elysiajs/cors';\napp.use(cors({ origin: /example\\.com/ }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_anchored_regex() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ origin: /^https:\\/\\/example\\.com$/ }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.use(cors({ origin: /example\\.com/ }));";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
