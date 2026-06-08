//! zod-prefer-top-level-format backend — flag `z.string().email()`,
//! `z.string().url()`, `z.string().uuid()`, `z.number().int()`.
//!
//! Why: Zod v4 exposes top-level format functions (`z.email()`, `z.url()`,
//! `z.uuid()`, `z.int()`, `z.iso.datetime()`) that are shorter, faster,
//! and tree-shakeable compared to the `.string().method()` chain.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["call_expression"];

const STRING_CHAIN_METHODS: &[(&str, &str)] = &[
    ("email", "z.email()"),
    ("url", "z.url()"),
    ("uuid", "z.uuid()"),
    ("cuid", "z.cuid()"),
    ("ulid", "z.ulid()"),
    ("datetime", "z.iso.datetime()"),
    ("date", "z.iso.date()"),
    ("time", "z.iso.time()"),
    ("ip", "z.ipv4() or z.ipv6()"),
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "member_expression" {
            return;
        }
        let Some(property) = function.child_by_field_name("property") else {
            return;
        };
        let Some(object) = function.child_by_field_name("object") else {
            return;
        };
        let Ok(method_name) = property.utf8_text(source_bytes) else {
            return;
        };

        // Check z.string().<method>()
        if let Some((_, replacement)) = STRING_CHAIN_METHODS.iter().find(|(m, _)| *m == method_name)
            && is_z_string_call(object, source_bytes)
        {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "zod-prefer-top-level-format".into(),
                message: format!(
                    "`z.string().{method_name}()` — use `{replacement}` \
                     directly. Top-level format helpers are shorter, \
                     faster, and tree-shakeable."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        // Check z.number().int()
        if method_name == "int" && is_z_number_call(object, source_bytes) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "zod-prefer-top-level-format".into(),
                message: "`z.number().int()` — use `z.int()` directly.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn is_z_string_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    is_z_method_call(node, "string", source)
}

fn is_z_number_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    is_z_method_call(node, "number", source)
}

fn is_z_method_call(node: tree_sitter::Node, method: &str, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(function) = node.child_by_field_name("function") else {
        return false;
    };
    function
        .utf8_text(source)
        .is_ok_and(|t| t == format!("z.{method}"))
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
    fn flags_string_email() {
        assert_eq!(run_on("const s = z.string().email();").len(), 1);
    }

    #[test]
    fn flags_string_url() {
        assert_eq!(run_on("const s = z.string().url();").len(), 1);
    }

    #[test]
    fn flags_number_int() {
        assert_eq!(run_on("const s = z.number().int();").len(), 1);
    }

    #[test]
    fn allows_top_level_format() {
        assert!(run_on("const s = z.email();").is_empty());
        assert!(run_on("const s = z.int();").is_empty());
    }

    #[test]
    fn allows_plain_string_schema() {
        assert!(run_on("const s = z.string();").is_empty());
    }
}
