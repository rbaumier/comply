//! react-no-html-entities-in-jsx oxc backend.
//!
//! Walks JSX text nodes and string-valued attribute values. Flags useless
//! HTML entities — those whose raw character is rendered identically by
//! React. `&lt;` and `&nbsp;` are excluded because there is no readable raw
//! equivalent in JSX text. `&amp;` that is followed by what would itself be
//! a real entity (e.g. `&amp;copy;`) is treated as legitimate — the leading
//! `&` genuinely needs to be encoded there to avoid being parsed as `©`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::JSXAttributeValue;
use std::sync::Arc;

pub struct Check;

const NAMED_ENTITIES: &[(&str, char)] = &[
    ("apos", '\''),
    ("quot", '"'),
    ("amp", '&'),
    ("gt", '>'),
];

const NUMERIC_ENTITIES: &[(u32, char)] = &[
    (39, '\''),
    (34, '"'),
    (38, '&'),
    (62, '>'),
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        // Empty → engine calls `run_on_semantic`, which iterates JSXText
        // and JSXAttribute nodes ourselves.
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // JSXText.value and StringLiteral.value are the *parsed* values
        // (entities already decoded). Scan the raw source slice instead.
        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::JSXText(text) => {
                    let raw = source_slice(ctx.source, text.span.start, text.span.end);
                    if let Some((entity, replacement)) = find_useless_entity(raw) {
                        emit(
                            &mut diagnostics,
                            ctx,
                            text.span.start as usize,
                            &entity,
                            replacement,
                        );
                    }
                }
                AstKind::JSXAttribute(attr) => {
                    let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
                        continue;
                    };
                    let raw = source_slice(ctx.source, lit.span.start, lit.span.end);
                    if let Some((entity, replacement)) = find_useless_entity(raw) {
                        emit(
                            &mut diagnostics,
                            ctx,
                            lit.span.start as usize,
                            &entity,
                            replacement,
                        );
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

fn source_slice(source: &str, start: u32, end: u32) -> &str {
    let s = start as usize;
    let e = (end as usize).min(source.len());
    if s >= e { return ""; }
    &source[s..e]
}

fn emit(
    diagnostics: &mut Vec<Diagnostic>,
    ctx: &CheckCtx,
    offset: usize,
    entity: &str,
    replacement: char,
) {
    let (line, column) = byte_offset_to_line_col(ctx.source, offset);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "Useless HTML entity `{entity}` in JSX — use `{replacement}` directly. \
             React renders the raw character."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

/// Return the first useless entity found in `text`, as
/// (entity_text, replacement_char). `&amp;` followed by what would be a
/// real entity is skipped (the `&` there is genuinely escaping).
fn find_useless_entity(text: &str) -> Option<(String, char)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&'
            && let Some((end, replacement)) = scan_entity(text, i)
        {
            if replacement == '&' && looks_like_entity(text, end) {
                i = end;
                continue;
            }
            return Some((text[i..end].to_string(), replacement));
        }
        i += 1;
    }
    None
}

/// Parse a useless entity starting at `start` (where `text[start] == '&'`).
/// Returns `Some((end_byte, replacement_char))` where `end_byte` is one past
/// the closing `;`.
fn scan_entity(text: &str, start: usize) -> Option<(usize, char)> {
    let bytes = text.as_bytes();
    // Search up to 12 bytes for the closing ';' — enough for "#x000027;".
    let max = (start + 12).min(bytes.len());
    let semi = (start + 1..max).find(|&i| bytes[i] == b';')?;
    let name = &text[start + 1..semi];

    if let Some(rest) = name.strip_prefix('#') {
        let (radix, digits) = if let Some(hex) = rest.strip_prefix(['x', 'X']) {
            (16, hex)
        } else {
            (10, rest)
        };
        if digits.is_empty() {
            return None;
        }
        let num = u32::from_str_radix(digits, radix).ok()?;
        for (n, r) in NUMERIC_ENTITIES {
            if num == *n {
                return Some((semi + 1, *r));
            }
        }
        return None;
    }

    for (n, r) in NAMED_ENTITIES {
        if name == *n {
            return Some((semi + 1, *r));
        }
    }
    None
}

/// True when `text[from..]` starts with chars that form a valid entity
/// (letters/digits/`#`, then `;`). Used to recognise `&amp;copy;` and
/// similar shapes as legitimate escapes of a leading `&`.
fn looks_like_entity(text: &str, from: usize) -> bool {
    let bytes = text.as_bytes();
    let mut i = from;
    let mut content = false;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_alphanumeric() || b == b'#' {
            content = true;
            i += 1;
        } else if b == b';' {
            return content;
        } else {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_tsx;

    fn run(src: &str) -> Vec<Diagnostic> {
        run_oxc_tsx(src, &Check)
    }

    #[test]
    fn flags_apos_in_text() {
        let src = r#"const c = <p>L&apos;utilisateur</p>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_quot_in_text() {
        let src = r#"const c = <p>She said &quot;hi&quot;</p>;"#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn flags_amp_in_text() {
        let src = r#"const c = <p>Tom &amp; Jerry</p>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_gt_in_text() {
        let src = r#"const c = <p>3 &gt; 2</p>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_decimal_numeric_entity() {
        let src = r#"const c = <p>L&#39;ami</p>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_hex_numeric_entity() {
        let src = r#"const c = <p>L&#x27;ami</p>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_apos_in_attribute() {
        let src = r#"const c = <input title="L&apos;utilisateur" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_lt_entity_in_text() {
        // `&lt;` is legitimate — JSX text cannot contain a raw `<`.
        let src = r#"const c = <p>3 &lt; 2</p>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_nbsp_in_text() {
        // `&nbsp;` has no readable raw equivalent.
        let src = r#"const c = <p>foo&nbsp;bar</p>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unknown_named_entity() {
        // `&copy;` (©) etc. are kept as-is — they map to characters that
        // have no convenient JSX text representation.
        let src = r#"const c = <p>Copyright &copy; 2026</p>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_amp_before_real_entity() {
        // The literal text `&copy;` is rendered by escaping the leading `&`.
        // Removing the `&amp;` here would change behaviour to render `©`.
        let src = r#"const c = <p>Type &amp;copy; for copyright</p>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_amp_before_real_numeric_entity() {
        let src = r#"const c = <p>Code: &amp;#39;</p>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_amp_when_not_escaping_an_entity() {
        let src = r#"const c = <p>cats &amp; dogs</p>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_plain_text() {
        let src = r#"const c = <p>Hello world</p>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_jsx_expression_string() {
        // String inside a `{ ... }` expression is plain TS, not JSX text.
        // The rule should not look there.
        let src = r#"const c = <p>{"L'utilisateur with &apos; in code"}</p>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn reports_once_per_text_node() {
        let src = r#"const c = <p>L&apos;ami &quot;Bob&quot;</p>;"#;
        // Multiple entities in one text node → one diagnostic. Subsequent
        // fixes happen across reruns.
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_truncated_entity() {
        // `&apos` with no closing `;` is not an entity.
        let src = r#"const c = <p>L&apos no semi</p>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_useless_numeric_entity() {
        // `&#169;` is `©` — has no raw equivalent in plain text the way
        // `&#39;` does. Keep it.
        let src = r#"const c = <p>&#169; 2026</p>;"#;
        assert!(run(src).is_empty());
    }
}
