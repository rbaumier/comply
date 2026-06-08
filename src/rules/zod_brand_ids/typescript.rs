//! zod-brand-ids backend — suggest `.brand<"...">()` on ID-like fields.
//!
//! Fires on `pair` nodes inside an object literal (typically the argument
//! of `z.object({...})`) where:
//!
//! 1. the key is `id` or matches `*Id` / `*_id` (at least one character
//!    before the `Id` suffix),
//! 2. the value text starts with `z.` (i.e. it is a Zod schema), and
//! 3. the value text does not already contain `.brand(`.
//!
//! The check stays local to the pair — no whole-file analysis — which
//! keeps it O(n) on the AST and prevents cross-file false positives.

use crate::diagnostic::{Diagnostic, Severity};

/// Return `true` if `key` is an ID-like field name.
///
/// Matches `id` (case-insensitive) and any camelCase / snake_case name
/// whose last segment is `Id` / `id`, e.g. `userId`, `post_id`, `authorID`.
fn is_id_like(key: &str) -> bool {
    let key = key.trim_matches(|c: char| c == '"' || c == '\'');
    if key.eq_ignore_ascii_case("id") {
        return true;
    }
    // snake_case: must have a prefix before `_id`.
    if key.strip_suffix("_id").is_some_and(|p| !p.is_empty()) {
        return true;
    }
    if key.strip_suffix("_ID").is_some_and(|p| !p.is_empty()) {
        return true;
    }
    // camelCase: ends with `Id` / `ID`, prefix non-empty and lowercase-ish
    // (avoid matching `VALID`, `HYBRID`, …).
    for suffix in ["Id", "ID"] {
        if let Some(prefix) = key.strip_suffix(suffix) {
            if prefix.is_empty() {
                continue;
            }
            let last = prefix.chars().next_back().unwrap_or(' ');
            if last.is_ascii_lowercase() || last.is_ascii_digit() {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key_node) = node.child_by_field_name("key") else { return };
    let Some(value_node) = node.child_by_field_name("value") else { return };

    let Ok(key_text) = key_node.utf8_text(source) else { return };
    if !is_id_like(key_text) {
        return;
    }

    let Ok(value_text) = value_node.utf8_text(source) else { return };
    // Only schemas that start with a Zod call — skip non-schema values.
    if !value_text.starts_with("z.") {
        return;
    }
    // `.brand(` covers the runtime call form; `.brand<` covers the
    // typed form `.brand<"UserId">()`. Either one signals the author
    // already opted into branding.
    if value_text.contains(".brand(") || value_text.contains(".brand<") {
        return;
    }

    let pos = key_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "zod-brand-ids".into(),
        message: format!(
            "`{}` looks like an ID — add `.brand<\"...\">()` so distinct IDs \
             are not assignable to each other.",
            key_text.trim_matches(|c: char| c == '"' || c == '\''),
        ),
        severity: Severity::Warning,
        span: None,
    });
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
    fn flags_bare_id_field() {
        let d = run_on("const S = z.object({ id: z.string() });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_user_id_camel() {
        let d = run_on("const S = z.object({ userId: z.string().uuid() });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_post_id_snake() {
        let d = run_on("const S = z.object({ post_id: z.string() });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_branded_id() {
        assert!(
            run_on("const S = z.object({ userId: z.string().brand<\"UserId\">() });",).is_empty()
        );
    }

    #[test]
    fn ignores_non_id_fields() {
        assert!(run_on("const S = z.object({ name: z.string() });").is_empty());
    }

    #[test]
    fn ignores_words_ending_in_caps_id() {
        // `VALID` ends in `ID` but is not an ID-like field.
        assert!(run_on("const S = z.object({ VALID: z.string() });").is_empty());
    }

    #[test]
    fn ignores_non_zod_values() {
        assert!(run_on("const obj = { userId: \"abc\" };").is_empty());
    }

    #[test]
    fn flags_multiple_ids() {
        let d = run_on("const S = z.object({ userId: z.string(), postId: z.string() });");
        assert_eq!(d.len(), 2);
    }
}
