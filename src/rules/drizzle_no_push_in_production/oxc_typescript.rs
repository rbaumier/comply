//! drizzle-no-push-in-production OxcCheck backend — flag `drizzle-kit push`
//! inside string and template literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const NEEDLE: &str = "drizzle-kit push";

fn find_push(text: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find(NEEDLE) {
        let abs = search_from + rel;
        let after = abs + NEEDLE.len();
        let ok = match text.as_bytes().get(after) {
            None => true,
            Some(b) => matches!(*b, b':' | b' ' | b'\t' | b'\n' | b'"' | b'\'' | b'`'),
        };
        if ok {
            return Some(abs);
        }
        search_from = after;
    }
    None
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["drizzle-kit push"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::StringLiteral(lit) => {
                    if find_push(&lit.value).is_some() {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, lit.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "`drizzle-kit push` bypasses migrations — use \
                                      `drizzle-kit generate` + `drizzle-kit migrate` in CI \
                                      and production deployments."
                                .into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                }
                AstKind::TemplateLiteral(tpl) => {
                    for quasi in &tpl.quasis {
                        let raw = quasi.value.raw.as_str();
                        if find_push(raw).is_some() {
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, quasi.span.start as usize);
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line,
                                column,
                                rule_id: super::META.id.into(),
                                message: "`drizzle-kit push` bypasses migrations — use \
                                          `drizzle-kit generate` + `drizzle-kit migrate` in CI \
                                          and production deployments."
                                    .into(),
                                severity: Severity::Error,
                                span: None,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }


    #[test]
    fn flags_plain_push_in_string() {
        assert_eq!(run_on("const cmd = \"drizzle-kit push\";").len(), 1);
    }


    #[test]
    fn flags_dialect_suffixed_push_in_template() {
        assert_eq!(
            run_on("const cmd = `drizzle-kit push:pg --config=drizzle.config.ts`;").len(),
            1
        );
    }


    #[test]
    fn flags_push_in_object_literal() {
        assert_eq!(
            run_on("const scripts = { deploy: 'drizzle-kit push' };").len(),
            1
        );
    }


    #[test]
    fn allows_drizzle_kit_migrate() {
        assert!(run_on("const cmd = \"drizzle-kit migrate\";").is_empty());
    }


    #[test]
    fn allows_pusher_word() {
        // `drizzle-kit pusher` is not a command we care about.
        assert!(run_on("const cmd = \"drizzle-kit pusher\";").is_empty());
    }


    #[test]
    fn allows_push_outside_string() {
        // A bare identifier `push` in code shouldn't be flagged.
        assert!(run_on("queue.push(item);").is_empty());
    }
}
