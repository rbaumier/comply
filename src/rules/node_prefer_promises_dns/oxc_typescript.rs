use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const DNS_METHODS: &[&str] = &[
    "lookup",
    "lookupService",
    "resolve",
    "resolve4",
    "resolve6",
    "resolveAny",
    "resolveCname",
    "resolveMx",
    "resolveNaptr",
    "resolveNs",
    "resolvePtr",
    "resolveSoa",
    "resolveSrv",
    "resolveTxt",
    "reverse",
    "getServers",
    "setServers",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["dns"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };

        let method = member.property.name.as_str();
        if !DNS_METHODS.contains(&method) {
            return;
        }

        // Object must be bare `dns` identifier (not `dns.promises`).
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "dns" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "node-prefer-promises-dns".into(),
            message: format!("Use `dns.promises.{method}()` instead of callback-based `dns.{method}()`."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
