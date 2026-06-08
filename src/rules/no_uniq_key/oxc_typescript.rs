//! no-uniq-key oxc backend — flag non-unique keys in JSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeName;
use std::sync::Arc;

/// Non-stable key generators that produce a new value every render.
const BAD_KEY_CALLS: &[&str] = &["Math.random", "Date.now", "uuid", "uuidv4", "nanoid"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else {
            return;
        };
        let JSXAttributeName::Identifier(ident) = &attr.name else {
            return;
        };
        if ident.name.as_str() != "key" {
            return;
        }
        // Get the full text of the attribute value to check for bad key calls.
        let text = &ctx.source[attr.span.start as usize..attr.span.end as usize];
        if !BAD_KEY_CALLS.iter().any(|pat| text.contains(pat)) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Non-unique key \u{2014} `Math.random()`, `Date.now()`, or `uuid()` create new keys every render, breaking reconciliation.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    #[test]
    fn flags_math_random_key() {
        let d = crate::rules::test_helpers::run_oxc_tsx(
            r#"const el = <Item key={Math.random()} />;"#,
            &Check,
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-uniq-key");
    }


    #[test]
    fn flags_date_now_key() {
        let d =
            crate::rules::test_helpers::run_oxc_tsx(r#"const el = <Item key={Date.now()} />;"#, &Check);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_uuid_key() {
        let d = crate::rules::test_helpers::run_oxc_tsx(r#"const el = <Item key={uuid()} />;"#, &Check);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_stable_key() {
        let d =
            crate::rules::test_helpers::run_oxc_tsx(r#"const el = <Item key={item.id} />;"#, &Check);
        assert!(d.is_empty());
    }


    #[test]
    fn allows_index_key() {
        let d = crate::rules::test_helpers::run_oxc_tsx(r#"const el = <Item key={index} />;"#, &Check);
        assert!(d.is_empty());
    }
}
