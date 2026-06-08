//! no-prototype-pollution — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const MERGE_CALLS: &[&str] = &[
    "_.merge",
    "lodash.merge",
    "deepMerge",
    "mergeDeep",
    "Object.assign",
];

const USER_DATA_NEEDLES: &[&str] = &["req.body", "request.body", "JSON.parse"];

fn looks_like_user_data(text: &str) -> bool {
    USER_DATA_NEEDLES.iter().any(|n| text.contains(n))
}

/// Extract the callee name from a CallExpression as a dotted string.
fn callee_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::StaticMemberExpression(member) => {
            let obj_name = callee_name(&member.object)?;
            Some(format!("{}.{}", obj_name, member.property.name))
        }
        _ => None,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["deepMerge", "mergeDeep"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Some(name) = callee_name(&call.callee) else { return };
        let matches_merge = MERGE_CALLS.iter().any(|m| name == *m || name.ends_with(&format!(".{m}")));
        if !matches_merge {
            return;
        }

        let mut tainted = false;
        for arg in &call.arguments {
            let arg_start = arg.span().start as usize;
            let arg_end = arg.span().end as usize;
            let text = ctx.source.get(arg_start..arg_end).unwrap_or("");
            if looks_like_user_data(text) {
                tainted = true;
                break;
            }
        }
        if !tainted {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Deep-merging user-controlled data risks prototype pollution \u{2014} sanitize input before merging.".into(),
            severity: Severity::Error,
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
    fn flags_lodash_merge_req_body() {
        assert_eq!(run_on("_.merge(config, req.body)").len(), 1);
    }


    #[test]
    fn flags_merge_with_json_parse() {
        assert_eq!(run_on("deepMerge(defaults, JSON.parse(raw))").len(), 1);
    }


    #[test]
    fn flags_object_assign_req_body() {
        assert_eq!(run_on("Object.assign(target, req.body)").len(), 1);
    }


    #[test]
    fn allows_merge_safe_data() {
        assert!(run_on("_.merge(config, defaults)").is_empty());
    }


    #[test]
    fn allows_unrelated_call() {
        assert!(run_on("add(a, req.body)").is_empty());
    }
}
