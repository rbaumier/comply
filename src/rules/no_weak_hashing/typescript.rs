//! no-weak-hashing backend — flag weak hash algorithms (MD5, SHA-1).

use crate::diagnostic::{Diagnostic, Severity};

const WEAK_ALGOS: &[&str] = &["md5", "sha1"];

/// Extract the inner text of a string node (strip quotes).
fn string_inner<'a>(node: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    let text = node.utf8_text(source).unwrap_or("");
    // Strip surrounding quotes (' or " or `)
    if text.len() >= 2 {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    let Some(args) = node.child_by_field_name("arguments") else { return };

    // Match `createHash('md5')` / `createHash("sha1")` — direct or member call.
    let is_create_hash = match callee.kind() {
        "identifier" => callee.utf8_text(source).unwrap_or("") == "createHash",
        "member_expression" => {
            callee
                .child_by_field_name("property")
                .and_then(|p| p.utf8_text(source).ok())
                == Some("createHash")
        }
        _ => false,
    };

    if is_create_hash {
        // Check first argument for weak algo.
        for i in 0..args.named_child_count() {
            let Some(arg) = args.named_child(i) else { continue };
            if arg.kind() == "string" {
                let inner = string_inner(arg, source).to_ascii_lowercase();
                if WEAK_ALGOS.contains(&inner.as_str()) {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "no-weak-hashing".into(),
                        message: format!(
                            "Weak hashing algorithm `createHash('{}')` — use SHA-256 or stronger.",
                            inner,
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            break; // only check first arg
        }
        return;
    }

    // Match bare `MD5(...)` / `SHA1(...)` calls.
    let callee_name = match callee.kind() {
        "identifier" => callee.utf8_text(source).unwrap_or(""),
        _ => return,
    };

    if callee_name == "MD5" || callee_name == "SHA1" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-weak-hashing".into(),
            message: format!(
                "Weak hashing algorithm `{}` — use SHA-256 or stronger.",
                callee_name,
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_md5_single_quotes() {
        let d = run_on("const h = crypto.createHash('md5');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("md5"));
    }

    #[test]
    fn flags_sha1_double_quotes() {
        let d = run_on("const h = crypto.createHash(\"sha1\");");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("sha1"));
    }

    #[test]
    fn flags_md5_function() {
        let d = run_on("const hash = MD5(data);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_sha1_function() {
        let d = run_on("const hash = SHA1(data);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_sha256() {
        assert!(run_on("const h = crypto.createHash('sha256');").is_empty());
    }

    #[test]
    fn allows_non_hash_call() {
        assert!(run_on("const x = foo('md5');").is_empty());
    }
}
