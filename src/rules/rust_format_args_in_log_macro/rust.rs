//! rust-format-args-in-log-macro backend.
//!
//! For each log/tracing macro_invocation (`info!`, `debug!`, `warn!`,
//! `error!`, `trace!`), check whether its arguments are exactly the
//! redundant re-wrap shape `("{}", format!(...))`: a format-string literal
//! that is a single bare placeholder (`"{}"` / `"{:?}"`, no other text and
//! no second placeholder) whose sole remaining argument is a `format!`
//! invocation. tree-sitter-rust models macro arguments as an opaque
//! `token_tree`, so the arguments are parsed from the token-tree text.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{macro_body, split_top_level_args, string_literal_content};

const KINDS: &[&str] = &["macro_invocation"];

const LOG_MACROS: &[&str] = &["info", "debug", "warn", "error", "trace"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        let Some(macro_name_node) = node.child_by_field_name("macro") else {
            return;
        };
        let Ok(macro_name) = macro_name_node.utf8_text(source) else {
            return;
        };
        // Last segment for `tracing::info!` / `log::info!` style.
        let last_segment = macro_name.rsplit("::").next().unwrap_or(macro_name);
        if !LOG_MACROS.contains(&last_segment) {
            return;
        }
        let Ok(text) = node.utf8_text(source) else {
            return;
        };
        if !is_bare_placeholder_format_wrap(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "rust-format-args-in-log-macro",
            format!(
                "`{last_segment}!(\"{{}}\", format!(...))` double-formats. \
                 Pass the format args directly to `{last_segment}!` — log \
                 macros accept the same grammar as `format!`."
            ),
            Severity::Warning,
        ));
    }
}

/// True if the log macro invocation `text` (e.g. `info!("{}", format!(...))`)
/// has exactly the redundant re-wrap shape: two top-level arguments where the
/// first is a string literal that is a single bare placeholder (`"{}"` /
/// `"{:?}"` — no surrounding text, no second placeholder, escaped braces
/// `{{`/`}}` are literal text) and the second is a `format!(...)` invocation.
///
/// Any other shape — literal text around the placeholder, multiple
/// placeholders, extra arguments, or `format!` appearing only inside a string
/// literal — is not the double-format footgun and returns false.
fn is_bare_placeholder_format_wrap(text: &str) -> bool {
    let Some(body) = macro_body(text) else {
        return false;
    };
    let args = split_top_level_args(body);
    let [fmt_arg, value_arg] = args.as_slice() else {
        return false;
    };
    let Some(fmt) = string_literal_content(fmt_arg.trim()) else {
        return false;
    };
    is_single_bare_placeholder(&fmt) && is_format_invocation(value_arg.trim())
}

/// True if `fmt` is exactly one placeholder and nothing else: a single `{...}`
/// spanning the whole string, with no literal text before or after and no
/// second placeholder. The spec inside the braces may carry a format trait
/// (`{:?}`, `{:#?}`, `{:>8}`) but must not reference an argument by index or
/// name (`{0}`, `{x}`), which would not be a positional auto-numbered
/// placeholder consuming the trailing argument.
fn is_single_bare_placeholder(fmt: &str) -> bool {
    let Some(rest) = fmt.strip_prefix('{') else {
        return false;
    };
    let Some(inner) = rest.strip_suffix('}') else {
        return false;
    };
    // No nested braces: a single placeholder body never contains `{` or `}`
    // (escaped braces `{{`/`}}` as literal text would fail the strip above
    // anyway, since they leave surrounding text).
    if inner.contains('{') || inner.contains('}') {
        return false;
    }
    // The argument reference (text before any `:`) must be empty — an
    // auto-numbered positional placeholder. `{0}` / `{name}` are rejected.
    let arg_ref = inner.split_once(':').map_or(inner, |(a, _)| a);
    arg_ref.trim().is_empty()
}

/// True if `arg` is a `format!(...)` macro invocation: the `format` identifier
/// (bare or path-qualified, e.g. `std::format`) immediately followed by `!` and
/// a delimiter. A leading `&` (`&format!(...)`) is a borrow of a composite
/// value, not the bare wrapper, so it is not matched.
fn is_format_invocation(arg: &str) -> bool {
    let Some(bang) = arg.find('!') else {
        return false;
    };
    let path = arg[..bang].trim();
    let last_segment = path.rsplit("::").next().unwrap_or(path);
    if last_segment != "format" {
        return false;
    }
    arg[bang + 1..]
        .trim_start()
        .starts_with(['(', '[', '{'])
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_info_with_inner_format() {
        let src = r#"fn f() { info!("{}", format!("x={}", 1)); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_tracing_warn_with_inner_format() {
        let src = r#"fn f() { tracing::warn!("{}", format!("oops {}", e)); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_format_with_literal_text_around_placeholder() {
        // `"err: {}"` has literal text before the placeholder, so the `format!`
        // is not a redundant wrapper around the whole message — the rewrite the
        // diagnostic describes does not apply.
        let src = r#"fn f() { error!("err: {}", format!("{:?}", e)); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_plain_info() {
        let src = r#"fn f() { info!("x = {}", 1); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_format_outside_log() {
        let src = r#"fn f() { let s = format!("x = {}", 1); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_multi_placeholder_with_composite_format_arg() {
        // starship repro: the outer format string has two placeholders plus
        // literal text, and the inner `format!` builds one of several args
        // (a distinct composite string), not a redundant whole-message wrapper.
        let src = r#"fn f(pattern: &str, error: &str) {
            log::warn!(
                "Could not compile regular expression `{}`:\n{}",
                &format!("^{pattern}$"),
                error
            );
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_format_text_inside_string_literal() {
        // `format!` appears only inside the format-string literal — there is no
        // inner macro call at all.
        let src = r#"fn g() { log::warn!("avoid wrapping in format!() inside log macros"); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_debug_placeholder_with_text() {
        // A `{:?}` placeholder surrounded by literal text is not a bare wrapper.
        let src = r#"fn f() { warn!("oops: {:?}", format!("{}", e)); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_bare_debug_placeholder() {
        let src = r#"fn f() { warn!("{:?}", format!("x {}", e)); }"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
