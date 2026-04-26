//! jsdoc/check-values — validate `@version`, `@since`, `@license`.
//!
//! An empty or free-form value on these tags is a footgun: doc
//! generators read them verbatim and ship whatever you wrote. We
//! require a semver-ish string on version/since (must start with a
//! digit and contain at least one `.`) and a non-empty token on
//! license.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_helpers::scan_blocks;

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in scan_blocks(text) {
        for tag in block.tags() {
            match tag.name.as_str() {
                "version" | "since" => {
                    let body = tag.body.trim();
                    if !is_semverish(body) {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: tag.line + line_offset,
                            column: 1,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`@{}` value `{}` is not a semver-ish string (expected e.g. `1.2.3`).",
                                tag.name, body
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                "license" => {
                    let body = tag.body.trim();
                    if body.is_empty() {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: tag.line + line_offset,
                            column: 1,
                            rule_id: super::META.id.into(),
                            message:
                                "`@license` tag has no value — add an SPDX identifier like `MIT`."
                                    .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

/// Very lax semver check — `1`, `1.2`, `1.2.3`, `1.2.3-rc.1`, `v1.2`.
/// We accept any non-empty token that starts with an optional `v`
/// followed by a digit and contains only digits, dots, `-`, `+`,
/// `.`, and ASCII alphanumerics.
fn is_semverish(s: &str) -> bool {
    let mut chars = s.chars();
    let first = chars.next();
    let after_v = match first {
        Some('v') | Some('V') => chars.next(),
        Some(c) => Some(c),
        None => return false,
    };
    match after_v {
        Some(c) if c.is_ascii_digit() => {}
        _ => return false,
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_empty_version() {
        let src = "/**\n * @version\n */\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_free_form_version() {
        let src = "/**\n * @version latest\n */\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("semver"));
    }

    #[test]
    fn allows_valid_semver() {
        let src = "/**\n * @version 1.2.3\n * @since v0.9\n */\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_empty_license() {
        let src = "/**\n * @license\n */\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("SPDX"));
    }

    #[test]
    fn allows_spdx_license() {
        let src = "/**\n * @license MIT\n */\n";
        assert!(run(src).is_empty());
    }
}
