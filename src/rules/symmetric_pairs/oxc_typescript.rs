//! symmetric-pairs OxcCheck backend — flag exported functions with no
//! symmetric counterpart (get/set, add/remove, open/close, start/stop,
//! create/delete).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

/// Symmetric prefix pairs: (prefix, expected counterpart prefix).
const PAIRS: &[(&str, &str)] = &[
    ("set", "get"),
    ("add", "remove"),
    ("remove", "add"),
    ("open", "close"),
    ("close", "open"),
    ("start", "stop"),
    ("stop", "start"),
    ("create", "delete"),
    ("delete", "create"),
    ("create", "destroy"),
];

const PREFIXES: &[&str] = &[
    "get", "set", "add", "remove", "open", "close", "start", "stop", "create", "delete",
    "destroy",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        // We need the full program — use run_on_semantic.
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let nodes = semantic.nodes();

        // Collect all exported function names.
        let mut exports: Vec<(u32, String)> = Vec::new();

        for node in nodes.iter() {
            let oxc_ast::AstKind::ExportNamedDeclaration(export) = node.kind() else {
                continue;
            };
            let Some(ref decl) = export.declaration else {
                continue;
            };
            match decl {
                Declaration::FunctionDeclaration(f) => {
                    if let Some(ref id) = f.id {
                        exports.push((id.span.start, id.name.to_string()));
                    }
                }
                _ => {}
            }
        }

        let names: Vec<&str> = exports.iter().map(|(_, n)| n.as_str()).collect();

        for (offset, name) in &exports {
            if let Some((prefix, suffix)) = split_prefix(name) {
                let counterparts: Vec<&str> = PAIRS
                    .iter()
                    .filter(|(p, _)| *p == prefix)
                    .map(|(_, c)| *c)
                    .collect();

                if counterparts.is_empty() {
                    continue;
                }
                let has_pair = counterparts.iter().any(|cp| {
                    let expected = format!("{cp}{suffix}");
                    names.contains(&expected.as_str())
                });

                if !has_pair {
                    let expected_names: Vec<String> = counterparts
                        .iter()
                        .map(|cp| format!("{cp}{suffix}"))
                        .collect();
                    let (line, _column) =
                        byte_offset_to_line_col(ctx.source, *offset as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`export function {name}` has no symmetric counterpart — expected {}.",
                            expected_names.join(" or "),
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}

/// Split a function name into (prefix, suffix) if it matches a known prefix.
fn split_prefix(name: &str) -> Option<(&str, &str)> {
    for &pfx in PREFIXES {
        if name.len() > pfx.len() && name.starts_with(pfx) {
            let rest = &name[pfx.len()..];
            if rest.starts_with(|c: char| c.is_ascii_uppercase()) {
                return Some((pfx, rest));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn allows_getter_without_setter() {
        assert!(run_on("export function getFoo() {}").is_empty());
    }

    #[test]
    fn flags_setter_without_getter() {
        let d = run_on("export function setFoo() {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("getFoo"));
    }

    #[test]
    fn flags_open_without_close() {
        let d = run_on("export function openConnection() {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("closeConnection"));
    }

    #[test]
    fn flags_create_without_delete_or_destroy() {
        let d = run_on("export function createUser() {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("deleteUser") || d[0].message.contains("destroyUser"));
    }

    #[test]
    fn allows_create_with_destroy() {
        let src = "export function createUser() {}\nexport function destroyUser() {}";
        let d = run_on(src);
        assert!(!d.iter().any(|d| d.message.contains("createUser")));
    }

    #[test]
    fn ignores_non_exported_functions() {
        assert!(run_on("function getFoo() {}").is_empty());
    }
}
