//! Flag `<button|a|input>` elements whose Tailwind className forces a
//! sub-44-pixel footprint.
//!
//! Two heuristics fire:
//!   1. Explicit small dimensions — `h-4 w-4`, `size-3`, etc.
//!   2. Tiny padding paired with small text — `px-1 py-0.5 text-xs`. This
//!      almost always renders below 44 px tall/wide. We only flag when
//!      the className lacks an explicit `h-*` / `w-*` / `size-*` /
//!      `min-h-*` / `min-w-*` that could push the element back over the
//!      threshold.

use crate::diagnostic::{Diagnostic, Severity};

const TINY_SIZE_TOKENS: &[&str] = &[
    "h-0", "h-1", "h-2", "h-3", "h-4", "h-5", "h-6", "h-7", "h-8", "h-9", "h-10", "w-0", "w-1",
    "w-2", "w-3", "w-4", "w-5", "w-6", "w-7", "w-8", "w-9", "w-10", "size-0", "size-1", "size-2",
    "size-3", "size-4", "size-5", "size-6", "size-7", "size-8", "size-9", "size-10",
];

const INTERACTIVE_TAGS: &[&str] = &["button", "a", "input"];

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };
    if !INTERACTIVE_TAGS.contains(&tag) {
        return;
    }

    let mut class_value: Option<String> = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(name) = crate::rules::jsx::jsx_attribute_name(child, source) else { continue };
        if name != "className" { continue; }
        if let Some(s) = crate::rules::jsx::jsx_attribute_string_value(child, source) {
            class_value = Some(s.to_string());
        }
    }

    let Some(cls) = class_value else { return };
    let tokens: Vec<&str> = cls.split_ascii_whitespace().collect();
    let has_tiny_h = tokens.iter().any(|t| t.starts_with("h-") && TINY_SIZE_TOKENS.contains(t));
    let has_tiny_w = tokens.iter().any(|t| t.starts_with("w-") && TINY_SIZE_TOKENS.contains(t));
    let has_tiny_size = tokens.iter().any(|t| t.starts_with("size-") && TINY_SIZE_TOKENS.contains(t));

    // Padding-based smallness: `px-1 py-0.5 text-xs` style buttons that
    // never reach 44 px without explicit size utilities. We only flag
    // when no explicit sizing class exists that could rescue the height.
    const TINY_PADDING: &[&str] = &[
        "p-0", "p-0.5", "p-1",
        "px-0", "px-0.5", "px-1", "px-2",
        "py-0", "py-0.5", "py-1",
    ];
    const TINY_TEXT: &[&str] = &["text-xs", "text-sm"];
    let has_tiny_padding = tokens.iter().any(|t| TINY_PADDING.contains(t));
    let has_tiny_text = tokens.iter().any(|t| TINY_TEXT.contains(t));
    let has_explicit_size = tokens.iter().any(|t| {
        t.starts_with("h-")
            || t.starts_with("w-")
            || t.starts_with("size-")
            || t.starts_with("min-h-")
            || t.starts_with("min-w-")
    });
    let tiny_via_padding = has_tiny_padding && has_tiny_text && !has_explicit_size;

    let tiny = (has_tiny_h && has_tiny_w) || has_tiny_size || tiny_via_padding;
    if !tiny { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "<{tag}> has a tap area under 44×44 px (className `{cls}`) — add padding or grow the hit target."
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_small_button() {
        let src = r#"const x = <button className="h-4 w-4" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_small_size_anchor() {
        let src = r##"const x = <a className="size-3" href="#">x</a>;"##;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_44px_button() {
        let src = r#"const x = <button className="h-12 w-12" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_interactive() {
        let src = r#"const x = <div className="h-4 w-4" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_padding_based_small_button() {
        // `px-2 py-1 text-xs` without explicit sizing → almost certainly below 44px.
        let src = r#"const x = <button className="px-2 py-1 text-xs">x</button>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_padding_based_small_anchor() {
        let src = r##"const x = <a className="px-1 py-0.5 text-sm" href="#">x</a>;"##;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_padding_with_min_height() {
        // Tiny padding but explicit min-h rescues the hit area.
        let src = r#"const x = <button className="px-2 py-1 text-xs min-h-12">x</button>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_padding_with_explicit_height() {
        let src = r#"const x = <button className="px-2 py-1 text-xs h-12">x</button>;"#;
        assert!(run(src).is_empty());
    }
}
