//! no-deprecated-api — flag usage of deprecated Node.js/browser APIs.
//!
//! Matches `new_expression` for `new Buffer(...)` and
//! `call_expression` for deprecated function calls like
//! `url.parse()`, `require('domain')`, `fs.exists()`, etc.

use crate::diagnostic::{Diagnostic, Severity};

const DEPRECATED_REQUIRES: &[(&str, &str)] = &[
    ("domain", "The `domain` module is deprecated — use structured error handling instead."),
    ("punycode", "The `punycode` module is deprecated — use the userland `punycode` package."),
];

const DEPRECATED_MEMBER_CALLS: &[(&str, &str, &str)] = &[
    ("fs", "exists", "Use `fs.existsSync()`, `fs.stat()`, or `fs.access()` instead of `fs.exists()`."),
    ("url", "parse", "Use `new URL()` instead of `url.parse()`."),
    ("util", "isArray", "Use `Array.isArray()` instead of `util.isArray()`."),
    ("util", "pump", "Use `stream.pipeline()` or `.pipe()` instead of `util.pump()`."),
];

const DEPRECATED_MEMBER_ACCESS: &[(&str, &str, &str)] = &[
    ("querystring", "escape", "The `querystring` module is deprecated — use `URLSearchParams` instead."),
    ("process.env", "NODE_DEBUG", "Use the `util.debuglog()` API instead of reading `process.env.NODE_DEBUG` directly."),
];

crate::ast_check! { on ["new_expression", "call_expression", "member_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        "new_expression" => {
            let Some(constructor) = node.child_by_field_name("constructor") else { return };
            let Ok(name) = constructor.utf8_text(source) else { return };
            if name == "Buffer" {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-deprecated-api".into(),
                    message: "Use `Buffer.from()` or `Buffer.alloc()` instead of `new Buffer()`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        "call_expression" => {
            let Some(callee) = node.child_by_field_name("function") else { return };

            // Check require('deprecated-module')
            if let Ok(callee_text) = callee.utf8_text(source)
                && callee_text == "require" {
                    let Some(args) = node.child_by_field_name("arguments") else { return };
                    let Some(first_arg) = args.named_child(0) else { return };
                    if first_arg.kind() == "string" {
                        let Ok(raw) = first_arg.utf8_text(source) else { return };
                        let val = &raw[1..raw.len().saturating_sub(1)];
                        for &(module, message) in DEPRECATED_REQUIRES {
                            if val == module {
                                let pos = node.start_position();
                                diagnostics.push(Diagnostic {
                                    path: ctx.path.to_path_buf(),
                                    line: pos.row + 1,
                                    column: pos.column + 1,
                                    rule_id: "no-deprecated-api".into(),
                                    message: message.into(),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                            }
                        }
                    }
                    return;
                }

            // Check deprecated member calls like fs.exists(), url.parse()
            if callee.kind() == "member_expression" {
                let Some(obj) = callee.child_by_field_name("object") else { return };
                let Some(prop) = callee.child_by_field_name("property") else { return };
                let Ok(obj_text) = obj.utf8_text(source) else { return };
                let Ok(prop_text) = prop.utf8_text(source) else { return };

                for &(o, p, message) in DEPRECATED_MEMBER_CALLS {
                    if obj_text == o && prop_text == p {
                        let pos = node.start_position();
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: "no-deprecated-api".into(),
                            message: message.into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
        }
        "member_expression" => {
            // Check deprecated member access like querystring.escape, process.env.NODE_DEBUG
            let Some(obj) = node.child_by_field_name("object") else { return };
            let Some(prop) = node.child_by_field_name("property") else { return };
            let Ok(obj_text) = obj.utf8_text(source) else { return };
            let Ok(prop_text) = prop.utf8_text(source) else { return };

            for &(o, p, message) in DEPRECATED_MEMBER_ACCESS {
                if obj_text == o && prop_text == p {
                    // Skip if this member_expression is the callee of a call
                    // (already handled above)
                    if let Some(parent) = node.parent()
                        && parent.kind() == "call_expression"
                            && let Some(fn_node) = parent.child_by_field_name("function")
                                && fn_node.id() == node.id() {
                                    return;
                                }
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "no-deprecated-api".into(),
                        message: message.into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_new_buffer() {
        assert_eq!(run_on("const buf = new Buffer(10);").len(), 1);
    }

    #[test]
    fn flags_url_parse() {
        assert_eq!(run_on("const parsed = url.parse(myUrl);").len(), 1);
    }

    #[test]
    fn flags_require_domain() {
        assert_eq!(run_on("const d = require('domain');").len(), 1);
    }

    #[test]
    fn allows_buffer_from() {
        assert!(run_on("const buf = Buffer.from('hello');").is_empty());
    }

    #[test]
    fn allows_new_url() {
        assert!(run_on("const u = new URL(myUrl);").is_empty());
    }
}
