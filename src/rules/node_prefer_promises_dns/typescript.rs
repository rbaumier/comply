//! node-prefer-promises-dns backend — flag callback-based `dns.*()` calls.

use crate::diagnostic::{Diagnostic, Severity};

const DNS_METHODS: &[&str] = &[
    "lookup", "lookupService", "resolve", "resolve4", "resolve6",
    "resolveAny", "resolveCname", "resolveMx", "resolveNaptr", "resolveNs",
    "resolvePtr", "resolveSoa", "resolveSrv", "resolveTxt", "reverse",
    "getServers", "setServers",
];

crate::ast_check! { on ["call_expression"] prefilter = ["dns"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(obj) = callee.child_by_field_name("object") else { return };
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");

    if !DNS_METHODS.contains(&method) {
        return;
    }

    // `dns.method(...)` — object is `dns` identifier.
    // `dns.promises.method(...)` — object is `dns.promises` member_expression → skip.
    match obj.kind() {
        "identifier" => {
            if obj.utf8_text(source).unwrap_or("") != "dns" {
                return;
            }
        }
        _ => return,
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-prefer-promises-dns".into(),
        message: format!("Use `dns.promises.{method}()` instead of callback-based `dns.{method}()`."),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_dns_lookup() {
        let d = run_on("dns.lookup('example.com', cb);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("dns.promises.lookup"));
    }

    #[test]
    fn flags_dns_resolve() {
        assert_eq!(run_on("dns.resolve('example.com', cb);").len(), 1);
    }

    #[test]
    fn allows_dns_promises() {
        assert!(run_on("dns.promises.lookup('example.com');").is_empty());
    }

    #[test]
    fn allows_other_object() {
        assert!(run_on("myDns.lookup('example.com', cb);").is_empty());
    }
}
