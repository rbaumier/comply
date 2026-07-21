//! axum-cors-regex-unanchored backend.
//!
//! A `tower_http::cors::AllowOrigin::predicate(closure)` decides which origins a
//! `CorsLayer` accepts. When the closure matches the origin against a `Regex`
//! whose pattern has no trailing `$` anchor, the pattern also matches longer
//! strings: `^https://.*\.example\.com` accepts `https://good.example.com.attacker.com`,
//! a subdomain/suffix bypass.
//!
//! The rule keys on the `AllowOrigin::predicate` call (a `scoped_identifier`
//! whose final segment is `predicate` and whose type segment is `AllowOrigin`,
//! path-qualified receivers included). It considers only the regex the closure
//! matches the *origin* against — the closure's first parameter — and ignores a
//! regex matched against the second parameter (the request parts: path,
//! headers). It fires when that origin regex is a string literal that is not
//! end-anchored.
//!
//! The origin regex is found from each `<regex>.is_match(<arg>)` call whose
//! `<arg>` is derived from the origin parameter:
//!
//! 1. an inline receiver (`Regex::new(<literal>).unwrap()` /
//!    `RegexBuilder::new(<literal>)...`) yields its literal directly, and
//! 2. a plain-identifier receiver is resolved by name to its in-scope
//!    binding (`let` / `static` / `const`, including a
//!    `Lazy::new(|| Regex::new(<literal>))`). Resolution respects lexical scope:
//!    the nearest enclosing binding wins and, within a scope, the last one that
//!    precedes the use site (shadowing) wins — so a same-named binding in a
//!    sibling or shadowed scope cannot leak in.
//!
//! Only a string-literal pattern is judged: a pattern built from a runtime value
//! (`Regex::new(&pattern)`) cannot be proven unanchored and stays silent, as does
//! an already-anchored pattern (`^https://.*\.example\.com$` or one ending in the
//! stricter `\z`), a non-regex predicate, and any regex not matched against the
//! origin.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::string_literal_content;
use tree_sitter::Node;

/// Final segment of a path node: the `name` of a `scoped_identifier`, or the
/// whole text of a plain `identifier`.
fn path_tail<'a>(node: Node<'a>, source: &'a [u8]) -> &'a str {
    match node.kind() {
        "scoped_identifier" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or(""),
        _ => node.utf8_text(source).unwrap_or(""),
    }
}

/// `AllowOrigin::predicate` path, including a qualified `cors::AllowOrigin::predicate`.
fn is_allow_origin_predicate(func: Node, source: &[u8]) -> bool {
    func.kind() == "scoped_identifier"
        && path_tail(func, source) == "predicate"
        && func
            .child_by_field_name("path")
            .is_some_and(|p| path_tail(p, source) == "AllowOrigin")
}

/// First named child of `node`, or `None`.
fn first_named_child(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).next()
}

/// The pattern string of a `Regex::new(<literal>)` / `RegexBuilder::new(<literal>)`
/// call, or `None` when `call` is not that shape or the argument is not a bare
/// string literal.
fn regex_new_pattern(call: Node, source: &[u8]) -> Option<String> {
    if call.kind() != "call_expression" {
        return None;
    }
    let func = call.child_by_field_name("function")?;
    if func.kind() != "scoped_identifier" || path_tail(func, source) != "new" {
        return None;
    }
    let owner = func.child_by_field_name("path")?;
    if !matches!(path_tail(owner, source), "Regex" | "RegexBuilder") {
        return None;
    }
    let args = call.child_by_field_name("arguments")?;
    let first = first_named_child(args)?;
    string_literal_content(first.utf8_text(source).ok()?)
}

/// Depth-first walk of `root`'s subtree, collecting every `Regex::new` /
/// `RegexBuilder::new` literal pattern found within it.
fn collect_regex_patterns(root: Node, source: &[u8], out: &mut Vec<String>) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if let Some(pattern) = regex_new_pattern(node, source) {
            out.push(pattern);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// The name of a closure's first parameter — the origin binding in an
/// `AllowOrigin::predicate(|origin, parts| ..)` closure. `None` when the first
/// parameter is not a plain identifier (a `_` wildcard, a tuple pattern, …), in
/// which case the origin is not bound to a name the body can regex-match.
fn closure_origin_param(closure: Node, source: &[u8]) -> Option<String> {
    let params = closure.child_by_field_name("parameters")?;
    let mut cursor = params.walk();
    // The first parameter positionally — a `_` wildcard is an unnamed child, so
    // skip only the `|` / `,` punctuation rather than filtering to named nodes.
    let first = params
        .children(&mut cursor)
        .find(|c| !matches!(c.kind(), "|" | ","))?;
    let ident = match first.kind() {
        "identifier" => Some(first),
        "parameter" => first
            .child_by_field_name("pattern")
            .filter(|p| p.kind() == "identifier"),
        _ => None,
    };
    ident.and_then(|n| n.utf8_text(source).ok()).map(str::to_owned)
}

/// True when `arg`'s subtree references the `origin` identifier — i.e. the value
/// being matched is derived from the origin parameter (`o.as_bytes()`,
/// `origin.to_str()...`), not from the request parts.
fn arg_references_origin(arg: Node, origin: &str, source: &[u8]) -> bool {
    let mut stack = vec![arg];
    while let Some(node) = stack.pop() {
        if node.kind() == "identifier" && node.utf8_text(source) == Ok(origin) {
            return true;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// Walk the closure subtree for `<regex>.is_match(<arg>)` calls whose `<arg>` is
/// derived from the `origin` parameter, recording the regex each matches with: an
/// inline receiver goes straight to `patterns`, a plain-identifier receiver goes
/// to `idents` for scope resolution. A regex matched against anything other than
/// the origin (e.g. the request path) is ignored.
fn collect_origin_regex(
    closure: Node,
    origin: &str,
    source: &[u8],
    patterns: &mut Vec<String>,
    idents: &mut Vec<String>,
) {
    let mut stack = vec![closure];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression"
            && let Some(func) = node.child_by_field_name("function")
            && func.kind() == "field_expression"
            && func
                .child_by_field_name("field")
                .is_some_and(|f| f.utf8_text(source).unwrap_or("") == "is_match")
            && let Some(args) = node.child_by_field_name("arguments")
            && let Some(arg) = first_named_child(args)
            && arg_references_origin(arg, origin, source)
            && let Some(recv) = func.child_by_field_name("value")
        {
            if recv.kind() == "identifier" {
                if let Ok(name) = recv.utf8_text(source) {
                    idents.push(name.to_owned());
                }
            } else {
                collect_regex_patterns(recv, source, patterns);
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// Resolve `name` to the `Regex::new` literal(s) of its in-scope binding,
/// searching outward from `from` (the predicate call) through its ancestor
/// scopes. The nearest enclosing scope that binds `name` wins; within that
/// scope the last binding preceding the use site wins (lexical shadowing). A
/// `static` / `const` binds regardless of order (module scope), a `let` must
/// precede the use. A same-named binding in a sibling or shadowed scope cannot
/// leak in.
fn resolve_bound_regex_patterns(from: Node, name: &str, source: &[u8], out: &mut Vec<String>) {
    let use_start = from.start_byte();
    let mut scope = from.parent();
    while let Some(node) = scope {
        let mut chosen: Option<Node> = None;
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            let (bound, order_independent) = match child.kind() {
                "let_declaration" => (
                    child
                        .child_by_field_name("pattern")
                        .filter(|p| p.kind() == "identifier"),
                    false,
                ),
                "static_item" | "const_item" => (child.child_by_field_name("name"), true),
                _ => (None, false),
            };
            if let Some(bound) = bound
                && bound.utf8_text(source) == Ok(name)
                && (order_independent || child.start_byte() < use_start)
                && let Some(value) = child.child_by_field_name("value")
            {
                chosen = Some(value);
            }
        }
        if let Some(value) = chosen {
            collect_regex_patterns(value, source, out);
            return;
        }
        scope = node.parent();
    }
}

/// A pattern is end-anchored when it terminates in `$` or the stricter `\z`.
/// Anything else — including matching only a prefix — is unanchored. A trailing
/// anchor may sit inside group closers (`(?:^https://…$)`, per-branch
/// `(?:^a$)|(?:^b$)`), which are still end-anchored, so trailing `)` and
/// whitespace are stripped before judging.
fn is_unanchored(pattern: &str) -> bool {
    let trimmed = pattern.trim_end_matches([')', ' ', '\t', '\n', '\r']);
    !trimmed.is_empty() && !trimmed.ends_with('$') && !trimmed.ends_with("\\z")
}

crate::ast_check! { on ["call_expression"] prefilter = ["AllowOrigin"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if !is_allow_origin_predicate(func, source) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(closure) = args
        .named_children(&mut cursor)
        .find(|n| n.kind() == "closure_expression")
    else {
        return;
    };
    // The origin the predicate decides on is the closure's first parameter; only
    // a regex matched against *that* value is an origin regex, not one matched
    // against the request parts.
    let Some(origin) = closure_origin_param(closure, source) else { return };

    let mut patterns: Vec<String> = Vec::new();
    let mut idents: Vec<String> = Vec::new();
    collect_origin_regex(closure, &origin, source, &mut patterns, &mut idents);
    for ident in &idents {
        resolve_bound_regex_patterns(node, ident, source, &mut patterns);
    }

    if !patterns.iter().any(|p| is_unanchored(p)) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "CORS origin regex is not anchored with a trailing `$`: an unanchored pattern such as \
         `^https://.*\\.example\\.com` also matches `https://good.example.com.attacker.com`. \
         Anchor the full origin: `Regex::new(r\"^https://.*\\.example\\.com$\")`."
            .into(),
        Severity::Error,
    ));
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    // ── Positive: an unanchored origin regex behind `AllowOrigin::predicate` ──

    #[test]
    fn flags_named_regex_missing_trailing_anchor() {
        // The issue's canonical shape: a `Regex` bound to a `let` and matched in
        // the predicate closure via `re.is_match(..)`, pattern missing `$`.
        let src = r#"
            fn app() {
                let re = Regex::new(r"^https://.*\.example\.com").unwrap();
                let cors = CorsLayer::new()
                    .allow_origin(AllowOrigin::predicate(move |o, _| re.is_match(o.as_bytes())));
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_regex_missing_trailing_anchor() {
        let src = r#"
            fn app() {
                let cors = CorsLayer::new().allow_origin(AllowOrigin::predicate(|o, _| {
                    Regex::new(r"^https://.*\.example\.com").unwrap().is_match(o.as_bytes())
                }));
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_static_lazy_regex_missing_trailing_anchor() {
        // A module-level `static Lazy<Regex>` resolved by name from the predicate.
        let src = r#"
            static ORIGIN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^https://.*\.example\.com").unwrap());
            fn app() {
                let cors = CorsLayer::new()
                    .allow_origin(AllowOrigin::predicate(|o, _| ORIGIN_RE.is_match(o.as_bytes())));
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_qualified_allow_origin_predicate() {
        let src = r#"
            fn app() {
                let cors = CorsLayer::new().allow_origin(
                    tower_http::cors::AllowOrigin::predicate(|o, _| {
                        Regex::new(r"^https://.*\.example\.com").unwrap().is_match(o.as_bytes())
                    }),
                );
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_group_wrapped_without_trailing_anchor() {
        // Group closers are stripped, but there is no `$` before them, so this
        // alternation is still an unanchored suffix bypass.
        let src = r#"
            fn app() {
                let re = Regex::new(r"^(https://a\.example\.com|https://b\.example\.com)").unwrap();
                let cors = CorsLayer::new()
                    .allow_origin(AllowOrigin::predicate(move |o, _| re.is_match(o.as_bytes())));
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_regex_builder_missing_trailing_anchor() {
        let src = r#"
            fn app() {
                let re = RegexBuilder::new(r"^https://.*\.example\.com").build().unwrap();
                let cors = CorsLayer::new()
                    .allow_origin(AllowOrigin::predicate(move |o, _| re.is_match(o.as_bytes())));
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn resolves_each_binding_in_its_own_scope() {
        // Two functions each with a local `let re`: only the unanchored one
        // flags. A same-named binding in a sibling scope must not leak in.
        let src = r#"
            fn insecure() {
                let re = Regex::new(r"^https://.*\.example\.com").unwrap();
                let cors = CorsLayer::new()
                    .allow_origin(AllowOrigin::predicate(move |o, _| re.is_match(o.as_bytes())));
            }
            fn secure() {
                let re = Regex::new(r"^https://.*\.example\.com$").unwrap();
                let cors = CorsLayer::new()
                    .allow_origin(AllowOrigin::predicate(move |o, _| re.is_match(o.as_bytes())));
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: anchored / non-regex / non-origin shapes stay silent ───────

    #[test]
    fn allows_named_regex_with_trailing_anchor() {
        // The issue's canonical safe shape: same code, anchored pattern.
        let src = r#"
            fn app() {
                let re = Regex::new(r"^https://.*\.example\.com$").unwrap();
                let cors = CorsLayer::new()
                    .allow_origin(AllowOrigin::predicate(move |o, _| re.is_match(o.as_bytes())));
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_inline_regex_with_trailing_anchor() {
        let src = r#"
            fn app() {
                let cors = CorsLayer::new().allow_origin(AllowOrigin::predicate(|o, _| {
                    Regex::new(r"^https://.*\.example\.com$").unwrap().is_match(o.as_bytes())
                }));
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_regex_anchored_with_z_escape() {
        // `\z` is a stricter end anchor than `$`; it must not be flagged.
        let src = r#"
            fn app() {
                let re = Regex::new(r"^https://.*\.example\.com\z").unwrap();
                let cors = CorsLayer::new()
                    .allow_origin(AllowOrigin::predicate(move |o, _| re.is_match(o.as_bytes())));
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_regex_predicate() {
        // No regex at all — an allow-list membership check is not this rule's concern.
        let src = r#"
            fn app() {
                let cors = CorsLayer::new().allow_origin(
                    AllowOrigin::predicate(|o, _| ALLOWED.contains(&o.as_bytes())),
                );
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_runtime_pattern_in_predicate() {
        // The pattern is built from a runtime value, not a literal — unprovable,
        // so the rule stays silent rather than guessing.
        let src = r#"
            fn app() {
                let re = Regex::new(&origin_pattern).unwrap();
                let cors = CorsLayer::new()
                    .allow_origin(AllowOrigin::predicate(move |o, _| re.is_match(o.as_bytes())));
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unanchored_regex_outside_predicate() {
        // An unanchored regex not tied to `AllowOrigin::predicate` origin matching
        // is out of scope — the rule fires only on the CORS origin predicate.
        let src = r#"
            fn app() {
                let re = Regex::new(r"^https://.*\.example\.com").unwrap();
                let matched = re.is_match(some_url.as_bytes());
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_alloworigin_exact_and_list() {
        // Restricted `AllowOrigin` constructors are not `predicate` and stay silent.
        let exact = "fn app() { let cors = CorsLayer::new().allow_origin(AllowOrigin::exact(origin)); }";
        let list = "fn app() { let cors = CorsLayer::new().allow_origin(AllowOrigin::list([a, b])); }";
        assert!(run(exact).is_empty());
        assert!(run(list).is_empty());
    }

    #[test]
    fn allows_group_wrapped_anchored_regex() {
        // A fully-anchored pattern wrapped in a non-capturing group ends with `)`
        // but is still end-anchored; a per-branch anchored allowlist alternation
        // likewise. Neither is a bypass.
        let wrapped = r#"
            fn app() {
                let re = Regex::new(r"(?:^https://.*\.example\.com$)").unwrap();
                let cors = CorsLayer::new()
                    .allow_origin(AllowOrigin::predicate(move |o, _| re.is_match(o.as_bytes())));
            }
        "#;
        let alternation = r#"
            fn app() {
                let re = Regex::new(r"(?:^https://a\.example\.com$)|(?:^https://b\.example\.com$)").unwrap();
                let cors = CorsLayer::new()
                    .allow_origin(AllowOrigin::predicate(move |o, _| re.is_match(o.as_bytes())));
            }
        "#;
        assert!(run(wrapped).is_empty());
        assert!(run(alternation).is_empty());
    }

    #[test]
    fn allows_regex_matching_request_parts_not_origin() {
        // The predicate regex-matches the request path (its second parameter),
        // which is legitimately unanchored, while the origin is exact-allow-listed.
        // Only a regex applied to the origin parameter is this rule's concern.
        let src = r#"
            fn app() {
                let cors = CorsLayer::new().allow_origin(AllowOrigin::predicate(|origin, parts| {
                    let health = Regex::new(r"^/health").unwrap();
                    health.is_match(parts.uri().path().as_bytes())
                        || ALLOWED.contains(&origin.as_bytes())
                }));
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_shadowed_binding_uses_in_scope_regex() {
        // An earlier same-named unanchored `re` is shadowed before the predicate
        // by an anchored `re`; the closure captures the anchored one, so the rule
        // must resolve to it and stay silent.
        let src = r#"
            fn app() {
                let re = Regex::new(r"^/metrics").unwrap();
                let _ = re.is_match(b"/metrics/foo");
                let re = Regex::new(r"^https://.*\.example\.com$").unwrap();
                let cors = CorsLayer::new()
                    .allow_origin(AllowOrigin::predicate(move |o, _| re.is_match(o.as_bytes())));
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_predicate_on_unrelated_type() {
        // A `predicate` associated fn on some other type is not tower_http CORS.
        let src = r#"
            fn app() {
                let re = Regex::new(r"^https://.*\.example\.com").unwrap();
                let f = Filter::predicate(move |o, _| re.is_match(o));
            }
        "#;
        assert!(run(src).is_empty());
    }
}
