//! jsdoc/require-template OXC backend — comment-based, uses semantic.comments().

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, following_code, has_tag, parse_tags};
use std::sync::Arc;

pub struct Check;

/// Returns the contents of the signature's OWN type-parameter list (the text
/// between its `<` and matching `>`), or `None` when it declares none.
///
/// The own list is the `<...>` that opens before the top-level boundary
/// (`extends`/`implements`/`{`, or `=` for aliases). A first `<` that appears
/// after that boundary belongs to a superclass type argument (`extends
/// Base<T>`) or a return type, not to this signature.
fn extract_generics_between<'a>(code: &'a str) -> Option<&'a str> {
    let first_line = code
        .lines()
        .map(str::trim_start)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let head = match first_line.find('(') {
        Some(i) => &first_line[..i],
        None => first_line,
    };
    let open = head.find('<')?;
    if let Some(boundary) = own_params_boundary(head) {
        if open > boundary {
            return None;
        }
    }
    let close = open + matching_angle(&head[open..])?;
    let between = &head[open + 1..close];
    if between.trim().is_empty() {
        return None;
    }
    if between.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, ',' | ' ' | '_' | '=' | '|' | '{' | '}' | ':' | '<' | '>')
    }) {
        Some(between)
    } else {
        None
    }
}

/// Byte index of the earliest token that closes a signature's own
/// type-parameter list: the `extends`/`implements` keyword (whole word), or an
/// opening `{` or `=`. (`(` is already stripped from `head`.)
fn own_params_boundary(head: &str) -> Option<usize> {
    [
        find_keyword(head, "extends"),
        find_keyword(head, "implements"),
        head.find('{'),
        head.find('='),
    ]
    .into_iter()
    .flatten()
    .min()
}

/// Byte index, relative to `s`, of the `>` matching the `<` at `s[0]`,
/// honoring nested generics. `s` must begin with `<`.
fn matching_angle(s: &str) -> Option<usize> {
    let mut depth = 0u32;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Byte index of `keyword` matched as a whole word in `text`.
fn find_keyword(text: &str, keyword: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(rel) = text[from..].find(keyword) {
        let at = from + rel;
        let before_ok = at == 0 || !is_word_byte(bytes[at - 1]);
        let after = at + keyword.len();
        let after_ok = after >= bytes.len() || !is_word_byte(bytes[after]);
        if before_ok && after_ok {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Detect a `<T, U>` generics block in a function/class signature.
fn has_generics(code: &str) -> bool {
    extract_generics_between(code).is_some()
}

/// Returns true when every top-level type parameter has an `extends` constraint.
fn all_params_constrained(code: &str) -> bool {
    let Some(between) = extract_generics_between(code) else {
        return false;
    };
    let mut depth = 0usize;
    let mut start = 0;
    for (i, ch) in between.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if !between[start..i]
                    .split_whitespace()
                    .any(|w| w == "extends")
                {
                    return false;
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    between[start..]
        .split_whitespace()
        .any(|w| w == "extends")
}

fn starts_with_function_or_class(code: &str) -> bool {
    let first = code
        .lines()
        .map(str::trim_start)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    first.starts_with("function ")
        || first.starts_with("async function ")
        || first.starts_with("export function ")
        || first.starts_with("export async function ")
        || first.starts_with("export default function ")
        || first.starts_with("class ")
        || first.starts_with("export class ")
        || first.starts_with("export default class ")
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(text) = ctx.source.get(start..end) else {
                continue;
            };
            if !text.starts_with("/**") {
                continue;
            }

            let (line_offset, _) = byte_offset_to_line_col(ctx.source, start);
            // line_offset is 1-based from byte_offset_to_line_col, convert to 0-based for offset
            let line_offset = line_offset - 1;

            for block in find_jsdoc_blocks(text) {
                let tags = parse_tags(&block.content);
                if has_tag(&tags, "template") {
                    continue;
                }
                let code = following_code(ctx.source, text);
                if !starts_with_function_or_class(code) {
                    continue;
                }
                if !has_generics(code) {
                    continue;
                }
                if all_params_constrained(code) {
                    continue;
                }
                let (line, column) = (block.start_line + 1 + line_offset, 1);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message:
                        "Generic signature has no `@template` tag \u{2014} document each type parameter."
                            .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn ignores_non_generic_class_extending_generic_base() {
        // Regression for rbaumier/comply#6989 — the `<…>` belongs to the
        // superclass `Type`, not to `TinyIntType`, which has no own params.
        let source = r#"
/** Maps a database TINYINT column to a JS `number`. */
export class TinyIntType extends Type<number | null | undefined, number | null | undefined> {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_constrained_own_param_extending_generic_base() {
        // Regression for rbaumier/comply#6989 — own `<T extends string | number>`
        // is constrained, so it is exempt; the trailing `extends ArrayType<T>`
        // must not be mistaken for the own type-parameter list.
        let source = r#"
/** Wraps enum values in an array. */
export class EnumArrayType<T extends string | number = string> extends ArrayType<T> {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_non_generic_driver_extending_generic_base() {
        // Regression for rbaumier/comply#6989.
        let source = r#"
/** Postgres driver. */
export class PostgreSqlDriver extends AbstractSqlDriver<PostgreSqlConnection> {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_unconstrained_generic_class() {
        let source = r#"
/** A box. */
class Box<T> {}
"#;
        assert!(!run_on(source).is_empty());
    }

    #[test]
    fn flags_unconstrained_generic_class_extending_generic_base() {
        // The own `<T>` (before `extends`) is unconstrained; the superclass
        // type-arg `Base<T>` must not silence the diagnostic.
        let source = r#"
/** A wrapper. */
class Wrapper<T> extends Base<T> {}
"#;
        assert!(!run_on(source).is_empty());
    }

    #[test]
    fn flags_unconstrained_generic_function_with_generic_return() {
        // The own `<T>` is unconstrained; the `Box<T>` return-type `<` must not
        // be picked as the own type-parameter list.
        let source = r#"
/** Makes a box. */
function make<T>(): Box<T> {
  return new Box<T>();
}
"#;
        assert!(!run_on(source).is_empty());
    }

    #[test]
    fn ignores_constrained_generic_function() {
        let source = r#"
/** Identity. */
function id<T extends object>(x: T): T {
  return x;
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_nested_unconstrained_default_param() {
        // Own list with a nested default `<T = Bar<Baz>>`: the balanced `>`
        // finder must extract the full own list; T is unconstrained -> flags.
        let source = r#"
/** A holder. */
class Holder<T = Bar<Baz>> {}
"#;
        assert!(!run_on(source).is_empty());
    }
}
