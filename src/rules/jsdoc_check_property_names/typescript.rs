//! jsdoc/check-property-names — flag duplicate `@property` names.
//!
//! eslint-plugin-jsdoc rejects typedefs where two `@property` entries
//! share the same name because the resulting type is ambiguous: which
//! description wins? Which type wins? The source of truth is the
//! code, not the docs, and catching this here prevents silent drift.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_helpers::scan_blocks;
use rustc_hash::FxHashSet;

crate::ast_check! { on ["comment"] prefilter = ["/**"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in scan_blocks(text) {
        let tags = block.tags();
        let is_typedef = tags.iter().any(|t| {
            matches!(t.name.as_str(), "typedef" | "interface" | "namespace")
        });
        if !is_typedef {
            continue;
        }
        let mut seen: FxHashSet<String> = FxHashSet::default();
        for tag in tags
            .iter()
            .filter(|t| matches!(t.name.as_str(), "property" | "prop"))
        {
            let Some(name) = property_name(&tag.body) else {
                continue;
            };
            if !seen.insert(name.clone()) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: tag.line + line_offset,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Duplicate @property name `{name}` on a @typedef — remove or rename the duplicate."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

/// Extract the property name from a `@property` tag body. Body shapes
/// supported:
/// - `"{string} name - desc"` (type first)
/// - `"name - desc"` (no type)
/// - `"{string} name"` (no desc)
fn property_name(body: &str) -> Option<String> {
    let after_type = strip_leading_type(body);
    let first = after_type.split_whitespace().next()?;
    // An optional property is written `[name]` or `[name=default]` —
    // strip the brackets and any default value.
    let cleaned = first.trim_start_matches('[').trim_end_matches(']');
    let name = cleaned.split('=').next()?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// If `body` starts with `{...}`, return the rest (with leading
/// whitespace stripped). Otherwise return `body` unchanged.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_duplicate_property_names() {
        let src = r#"
/**
 * @typedef {Object} Point
 * @property {number} x - x
 * @property {number} x - duplicate
 */
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Duplicate"));
    }

    #[test]
    fn allows_unique_property_names() {
        let src = r#"
/**
 * @typedef {Object} Point
 * @property {number} x
 * @property {number} y
 */
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn only_policies_typedef_blocks() {
        let src = r#"
/**
 * @param {number} x
 * @param {number} x
 */
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn handles_optional_bracketed_names() {
        let src = r#"
/**
 * @typedef {Object} Opt
 * @property {number} [x]
 * @property {number} [x=1]
 */
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }
}
