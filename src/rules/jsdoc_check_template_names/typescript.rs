//! jsdoc/check-template-names — flag `@template T` entries whose `T`
//! is never referenced in another tag.
//!
//! An unreferenced type parameter in a JSDoc block is either dead
//! weight or a rename bug (the user renamed `T` → `U` in the code
//! but forgot the doc). eslint-plugin-jsdoc catches both cases.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_helpers::scan_blocks;

crate::ast_check! { on ["comment"] prefilter = ["/**"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in scan_blocks(text) {
        let tags = block.tags();
        // Collect every non-template tag body into one searchable
        // string — that's the haystack where `T` must appear.
        let haystack: String = tags
            .iter()
            .filter(|t| t.name != "template")
            .map(|t| t.body.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        for tag in tags.iter().filter(|t| t.name == "template") {
            // A `@template` body lists one or more names, possibly
            // with a constraint and/or description. Names are the
            // first word, optionally comma-separated.
            let names = extract_template_names(&tag.body);
            for name in names {
                if !contains_identifier(&haystack, &name) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: tag.line + line_offset,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "@template parameter `{name}` is declared but never referenced in the block."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
    }
}

/// Pull the identifier list out of a `@template` body. Accepted shapes:
/// - `"T"` → `["T"]`
/// - `"T, U"` → `["T", "U"]`
/// - `"T - description"` → `["T"]`
/// - `"{string} T"` → `["T"]`
fn extract_template_names(body: &str) -> Vec<String> {
    let after_type = strip_leading_type(body);
    // Keep only the portion before any separator that starts a
    // description (`-`, `:`).
    let head = after_type.split(['-', ':']).next().unwrap_or("");
    head.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && is_ident(s))
        .collect()
}

fn strip_leading_type(body: &str) -> &str {
    let trimmed = body.trim_start();
    if !trimmed.starts_with('{') {
        return trimmed;
    }
    let mut depth = 0usize;
    for (i, ch) in trimmed.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return trimmed[i + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    trimmed
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// Whole-word substring check (identifier-aware).
fn contains_identifier(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = hay.as_bytes();
    let n = needle.as_bytes();
    let mut i = 0;
    while i + n.len() <= bytes.len() {
        if &bytes[i..i + n.len()] == n {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after_idx = i + n.len();
            let after_ok = after_idx == bytes.len() || !is_ident_byte(bytes[after_idx]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_unused_template() {
        let src = r#"
/**
 * @template T
 * @param {string} x
 */
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`T`"));
    }

    #[test]
    fn allows_referenced_template() {
        let src = r#"
/**
 * @template T
 * @param {T} x
 * @returns {T}
 */
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn handles_comma_separated() {
        let src = r#"
/**
 * @template T, U
 * @param {T} x
 */
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`U`"));
    }

    #[test]
    fn whole_word_boundaries() {
        let src = r#"
/**
 * @template T
 * @param {SomeType} x
 */
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }
}
