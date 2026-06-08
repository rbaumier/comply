//! no-insecure-jwt AST backend — flag weak JWT algorithms (`none`, `HS256`)
//! in `jwt.verify(...)` / `jwt.sign(...)` option objects.
//!
//! Walks `call_expression` nodes whose callee is a member expression on a
//! `jwt`-named receiver (or a bare `verify`/`sign` JWT call), then inspects
//! the options object argument for `algorithm` / `algorithms` keys whose
//! string value (or array element) is `none` or `HS256`.

use crate::diagnostic::{Diagnostic, Severity};

/// Strip surrounding quotes from a string-literal node text.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c| c == '"' || c == '\'' || c == '`')
}

/// True if `node` is a `call_expression` whose function looks like a
/// JWT method invocation: `jwt.verify`, `jwt.sign`, `jsonwebtoken.verify`,
/// etc. (the receiver text simply has to contain `jwt`, case-insensitive).
fn is_jwt_call(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(object) = callee.child_by_field_name("object") else {
        return false;
    };
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    let Ok(method) = prop.utf8_text(source) else {
        return false;
    };
    if method != "verify" && method != "sign" && method != "decode" {
        return false;
    }
    let Ok(obj_text) = object.utf8_text(source) else {
        return false;
    };
    obj_text.to_ascii_lowercase().contains("jwt")
}

/// Scan an `object` AST node for an `algorithm`/`algorithms` pair whose
/// value is an insecure algorithm. Returns `Some((alg, is_array))` when
/// the violation is the algorithm string `alg`.
fn find_insecure_algorithm<'a>(obj: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    if obj.kind() != "object" {
        return None;
    }
    let mut cursor = obj.walk();
    for prop in obj.named_children(&mut cursor) {
        if prop.kind() != "pair" {
            continue;
        }
        let Some(key) = prop.child_by_field_name("key") else {
            continue;
        };
        let Ok(raw_key) = key.utf8_text(source) else {
            continue;
        };
        let key_name = unquote(raw_key);
        if key_name != "algorithm" && key_name != "algorithms" {
            continue;
        }
        let Some(value) = prop.child_by_field_name("value") else {
            continue;
        };
        if let Some(bad) = check_value_for_insecure(value, source) {
            return Some(bad);
        }
    }
    None
}

/// Check a value node (string literal, array of string literals) for an
/// insecure algorithm name. Returns the offending string slice.
fn check_value_for_insecure<'a>(value: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    match value.kind() {
        "string" => {
            let text = value.utf8_text(source).ok()?;
            let inner = unquote(text);
            if is_insecure_alg(inner) {
                Some(inner)
            } else {
                None
            }
        }
        "array" => {
            let mut cursor = value.walk();
            for el in value.named_children(&mut cursor) {
                if el.kind() == "string"
                    && let Ok(text) = el.utf8_text(source)
                {
                    let inner = unquote(text);
                    if is_insecure_alg(inner) {
                        return Some(inner);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn is_insecure_alg(s: &str) -> bool {
    s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("HS256")
}

crate::ast_check! { on ["call_expression"] prefilter = ["jwt", "JWT", "Jwt"] => |node, source, ctx, diagnostics|
    if !is_jwt_call(node, source) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    for arg in args.named_children(&mut cursor) {
        if let Some(bad) = find_insecure_algorithm(arg, source) {
            let message = if bad.eq_ignore_ascii_case("none") {
                "Insecure JWT algorithm `none` — use RS256 or ES256.".to_string()
            } else {
                "HS256 in JWT context — prefer asymmetric algorithms (RS256, ES256).".to_string()
            };
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                "no-insecure-jwt",
                message,
                Severity::Error,
            ));
            return;
        }
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
    fn flags_algorithm_none_single_quotes() {
        assert_eq!(
            run_on("jwt.verify(token, key, { algorithm: 'none' });").len(),
            1
        );
    }

    #[test]
    fn flags_algorithms_array_none() {
        assert_eq!(
            run_on("jwt.verify(token, key, { algorithms: ['none'] });").len(),
            1
        );
    }

    #[test]
    fn flags_hs256_in_jwt_context() {
        assert_eq!(
            run_on("jwt.sign(payload, secret, { algorithm: 'HS256' });").len(),
            1
        );
    }

    #[test]
    fn allows_rs256() {
        assert!(run_on("jwt.verify(token, key, { algorithm: 'RS256' });").is_empty());
    }

    #[test]
    fn allows_hs256_outside_jwt_context() {
        assert!(run_on("const algo = 'HS256';").is_empty());
    }
}
