//! rust-url-as-string backend.
//!
//! Walks `parameter` and `field_declaration` nodes and needs two independent
//! signals before it fires:
//!
//! 1. the declaration *names* a URL (`url`, `uri`, `endpoint`, `webhook`, or
//!    any `*_url` / `*_uri`) and *types* it as text — `String`, `str` behind a
//!    reference, `Cow<'_, str>`, `Box<str>`, or an `Option<…>` of those;
//! 2. that name undergoes string surgery: a trailing-slash trim
//!    (`trim_end_matches('/')`, `strip_suffix('/')`, `ends_with('/')`,
//!    `push('/')`), a scheme sniff (`starts_with("http")`), a `+ "/"`
//!    concatenation, or a `format!` that joins a path onto it (a `}/` in the
//!    format string). A parameter is judged on its own function's body, a
//!    struct field on the whole file — a field is read from methods far from
//!    its declaration, whereas a parameter that its function only forwards is
//!    not the one being patched.
//!
//! Signal 2 is what keeps the rule quiet on the legitimate cases: a wrapper
//! that forwards `&str` straight to `reqwest`, a `#[derive(Deserialize)]`
//! config struct, a `#[derive(clap::Parser)]` args struct. Passing a URL
//! around as text is fine; re-implementing URL syntax by hand is not.
//!
//! One diagnostic per declaration, anchored on the declaration rather than on
//! the surgery — the declaration is the type that has to change. Test code is
//! exempt.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{
    enclosing_fn, is_in_test_context, last_type_argument, strip_type_borrows,
};

/// Method-call surgery, matched on the text that immediately follows an
/// occurrence of the declared name. Each entry re-implements a rule of URL
/// syntax that `url::Url` already encodes.
const TRAILING_SURGERY: &[&str] = &[
    ".trim_end_matches('/')",
    ".trim_end_matches(\"/\")",
    ".trim_start_matches('/')",
    ".trim_start_matches(\"/\")",
    ".strip_suffix('/')",
    ".strip_suffix(\"/\")",
    ".ends_with('/')",
    ".ends_with(\"/\")",
    ".push_str(\"/",
    ".push('/')",
    ".starts_with(\"http",
];

/// How far past a `format!(` the balanced-paren scan is allowed to run. A
/// format call longer than this is not a URL join.
const MAX_FORMAT_SCAN: usize = 2_000;

crate::ast_check! { on ["parameter", "field_declaration"] prefilter = ["url", "uri", "endpoint", "webhook"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }
    if is_in_test_context(node, source) { return; }

    let name_field = if node.kind() == "parameter" { "pattern" } else { "name" };
    let Some(name_node) = node.child_by_field_name(name_field) else { return; };
    let Ok(name) = name_node.utf8_text(source) else { return; };
    if !is_url_name(name) { return; }

    let Some(type_node) = node.child_by_field_name("type") else { return; };
    let Ok(type_text) = type_node.utf8_text(source) else { return; };
    if !is_text_type(type_text) { return; }

    // A parameter is patched — or forwarded untouched — inside its own
    // function; reading the whole file would blame it for a namesake local in
    // another function.
    let scope = match node.kind() {
        "parameter" => enclosing_fn(node)
            .and_then(|function| function.utf8_text(source).ok())
            .unwrap_or(ctx.source),
        _ => ctx.source,
    };
    if !scope_does_url_surgery(scope, name) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{name}: {type_text}` holds a URL as text, and this file then rebuilds URL syntax by hand \
             (trailing-slash trimming, `format!` joins, scheme sniffing). Parse it into `url::Url` at the \
             boundary that receives it and use `Url::join` for sub-paths."
        ),
        Severity::Error,
    ));
}

/// True for a binding name that denotes a URL: the bare nouns, or any
/// `*_url` / `*_uri` compound (`base_url`, `callback_url`, `redirect_uri`).
fn is_url_name(name: &str) -> bool {
    matches!(name, "url" | "uri" | "endpoint" | "webhook")
        || name.ends_with("_url")
        || name.ends_with("_uri")
}

/// True for the textual types a URL gets smuggled through. `Url`, `http::Uri`
/// and anything else already parsed answers false — those are the target of the
/// fix, not its subject.
fn is_text_type(text: &str) -> bool {
    let stripped = strip_type_borrows(text);
    let head = stripped.split('<').next().unwrap_or(stripped).trim();
    let base = head.rsplit("::").next().unwrap_or(head).trim();
    match base {
        "String" | "str" => true,
        // `Cow<'a, str>`'s payload is its last argument; `Option<String>` and
        // `Box<str>` have only one.
        "Option" | "Cow" | "Box" => last_type_argument(stripped).is_some_and(is_text_type),
        _ => false,
    }
}

/// True when `name` is subjected to hand-written URL syntax anywhere in
/// `scope`. Textual scope, not lexical: a tree-sitter walk has no name
/// resolution, so the caller narrows the text handed in (a function body for a
/// parameter, the file for a field) rather than resolving bindings.
fn scope_does_url_surgery(scope: &str, name: &str) -> bool {
    let mut start = 0;
    while let Some(offset) = next_identifier_occurrence(scope, name, start) {
        let after = &scope[offset + name.len()..];
        if TRAILING_SURGERY.iter().any(|p| after.starts_with(p)) || concatenates_slash(after) {
            return true;
        }
        start = offset + name.len();
    }
    format_joins_path(scope, name)
}

/// True for a `<name> + "/"` / `<name> + '/'` concatenation, given the text
/// that follows the name.
fn concatenates_slash(after: &str) -> bool {
    let Some(rest) = after.trim_start().strip_prefix('+') else {
        return false;
    };
    let rest = rest.trim_start();
    rest.starts_with("\"/") || rest.starts_with("'/'")
}

/// True when a `format!` call in the file both mentions `name` and joins a path
/// onto a placeholder. `}/` is the signal: it is the tail of `{}/…` and of
/// `{url}/…` alike, and it cannot appear in a format string that is not
/// building a path.
fn format_joins_path(scope: &str, name: &str) -> bool {
    let mut start = 0;
    while let Some(offset) = scope[start..].find("format!(") {
        let open = start + offset + "format!(".len();
        let end = matching_paren(scope, open).unwrap_or(scope.len());
        let call = &scope[open..end];
        if call.contains("}/") && next_identifier_occurrence(call, name, 0).is_some() {
            return true;
        }
        start = open;
    }
    false
}

/// The byte offset of the `)` closing the paren that opened just before
/// `open`, or `None` when the scan runs past [`MAX_FORMAT_SCAN`] bytes without
/// balancing. Nested parens count; parens inside string literals are not
/// distinguished, which can only end the scan early on an unusual literal.
fn matching_paren(scope: &str, open: usize) -> Option<usize> {
    let mut depth = 1usize;
    for (index, c) in scope[open..].char_indices() {
        if index > MAX_FORMAT_SCAN {
            return None;
        }
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + index);
                }
            }
            _ => {}
        }
    }
    None
}

/// The offset of the next occurrence of `name` in `haystack` at or after
/// `from` that is a whole identifier — not a slice of a longer one, so a search
/// for `url` does not match `curl` or `url_parts`.
fn next_identifier_occurrence(haystack: &str, name: &str, from: usize) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let mut start = from;
    while let Some(offset) = haystack.get(start..)?.find(name) {
        let at = start + offset;
        let end = at + name.len();
        let before_ok = at == 0 || !is_identifier_byte(bytes[at - 1]);
        let after_ok = end >= bytes.len() || !is_identifier_byte(bytes[end]);
        if before_ok && after_ok {
            return Some(at);
        }
        start = end;
    }
    None
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_base_url_field_trimmed_of_trailing_slash() {
        let src = r#"
struct Client { base_url: String }

impl Client {
    fn get(&self, path: &str) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), path)
    }
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_url_parameter_joined_with_format() {
        let src = r#"
fn fetch(url: &str, path: &str) -> String {
    format!("{url}/{path}")
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_optional_endpoint_with_strip_suffix() {
        let src = r#"
struct Cfg { endpoint: Option<String> }

fn normalize(cfg: &Cfg) -> Option<&str> {
    cfg.endpoint.as_deref().and_then(|endpoint| endpoint.strip_suffix('/'))
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_cow_redirect_uri_with_starts_with_http() {
        let src = r#"
fn check(redirect_uri: Cow<'_, str>) -> bool {
    redirect_uri.starts_with("http")
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_webhook_concatenated_with_slash() {
        let src = r#"
fn build(webhook: String, path: String) -> String {
    webhook + "/" + &path
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn reports_once_per_declaration_not_per_usage() {
        let src = r#"
struct Client { base_url: String }

impl Client {
    fn a(&self) -> &str { self.base_url.trim_end_matches('/') }
    fn b(&self) -> &str { self.base_url.trim_end_matches('/') }
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_url_passed_through_without_surgery() {
        let src = r#"
fn fetch(url: &str) -> Result<Response, Error> {
    reqwest::blocking::get(url)
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_deserialize_config_struct_without_surgery() {
        let src = r#"
#[derive(Deserialize)]
struct Config { base_url: String, timeout_ms: u64 }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_clap_args_struct_without_surgery() {
        let src = r#"
#[derive(clap::Parser)]
struct Args {
    #[arg(long)]
    url: String,
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_already_parsed_url_type() {
        let src = r#"
struct Client { base_url: Url }

impl Client {
    fn get(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path)
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_http_uri_type() {
        let src = r#"
fn call(endpoint: http::Uri, path: &str) -> String {
    format!("{endpoint}/{path}")
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_url_name_with_surgery() {
        let src = r#"
fn strip(path: &str) -> &str {
    path.trim_end_matches('/')
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        let src = r#"
#[cfg(test)]
mod tests {
    fn fetch(url: &str) -> String { format!("{url}/x") }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_surgery_on_a_similarly_named_binding() {
        // `url_parts` is a different identifier; a substring match would have
        // read its surgery as the parameter's.
        let src = r#"
fn fetch(url: &str, url_parts: &str) -> String {
    url_parts.trim_end_matches('/').to_string()
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_parameter_forwarded_untouched_beside_a_patching_sibling_fn() {
        // The surgery lives in `join`, on `join`'s own `url`. `forward`'s
        // parameter is passed straight through and must stay unflagged.
        let src = r#"
fn forward(url: &str) -> Response { client.get(url).send() }

fn join(url: &str, path: &str) -> String { format!("{url}/{path}") }
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_format_without_a_path_join() {
        let src = r#"
fn label(url: &str) -> String {
    format!("fetching {url}")
}
"#;
        assert!(run(src).is_empty());
    }
}
