//! next-no-typos oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Declaration, ExportNamedDeclaration};
use std::sync::Arc;

pub struct Check;

const CANONICAL_NAMES: &[&str] = &[
    "getStaticProps",
    "getStaticPaths",
    "getServerSideProps",
    "getInitialProps",
];

/// Levenshtein-1 typo detection: matches if `name != canonical` but
/// differs by exactly one character substitution / addition / removal.
fn looks_like_typo_of(name: &str, canonical: &str) -> bool {
    if name == canonical {
        return false;
    }
    let nb = name.as_bytes();
    let cb = canonical.as_bytes();
    let nlen = nb.len();
    let clen = cb.len();

    if nlen.abs_diff(clen) > 1 {
        return false;
    }

    if nlen == clen {
        // Equal length — accept either a single substitution, or two
        // adjacent transposed bytes (`Porps` ↔ `Props`).
        let mut diff_positions: Vec<usize> = Vec::new();
        for (i, (a, b)) in nb.iter().zip(cb.iter()).enumerate() {
            if a != b {
                diff_positions.push(i);
                if diff_positions.len() > 2 {
                    return false;
                }
            }
        }
        return match diff_positions.len() {
            1 => true,
            2 => {
                let (p, q) = (diff_positions[0], diff_positions[1]);
                p + 1 == q && nb[p] == cb[q] && nb[q] == cb[p]
            }
            _ => false,
        };
    }
    // Lengths differ by 1 — check insert/delete pattern.
    let (short, long) = if nlen < clen { (nb, cb) } else { (cb, nb) };
    let mut i = 0;
    let mut j = 0;
    let mut diffs = 0;
    while i < short.len() && j < long.len() {
        if short[i] == long[j] {
            i += 1;
            j += 1;
        } else {
            diffs += 1;
            j += 1;
            if diffs > 1 {
                return false;
            }
        }
    }
    diffs + (long.len() - j) <= 1
}

fn check_decl_name(decl: &Declaration) -> Option<(String, &'static str, u32)> {
    match decl {
        Declaration::FunctionDeclaration(func) => {
            let id = func.id.as_ref()?;
            let name = id.name.as_str();
            for &canonical in CANONICAL_NAMES {
                if looks_like_typo_of(name, canonical) {
                    return Some((name.to_string(), canonical, id.span.start));
                }
            }
            None
        }
        Declaration::VariableDeclaration(var_decl) => {
            for declr in &var_decl.declarations {
                let id_name = match &declr.id {
                    oxc_ast::ast::BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
                    _ => None,
                };
                if let Some(name) = id_name {
                    for &canonical in CANONICAL_NAMES {
                        if looks_like_typo_of(name, canonical) {
                            return Some((name.to_string(), canonical, declr.span.start));
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportNamedDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ExportNamedDeclaration(export) = node.kind() else {
            return;
        };
        let _ = check_named_export(export, ctx, diagnostics);
    }
}

fn check_named_export(
    export: &ExportNamedDeclaration,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<()> {
    let decl = export.declaration.as_ref()?;
    let (name, canonical, span_start) = check_decl_name(decl)?;
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "`{name}` looks like a typo of Next.js's `{canonical}` — the data-fetching \
             hook will be silently ignored. Rename to `{canonical}`."
        ),
        severity: Severity::Error,
        span: None,
    });
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_get_static_porps_typo() {
        let src = "export async function getStaticPorps() { return { props: {} }; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_get_server_side_props_typo() {
        let src = "export const getServerSidePorps = async () => ({ props: {} });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_canonical_names() {
        let src = "export async function getStaticProps() { return { props: {} }; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_unrelated_exports() {
        let src = "export function loadConfig() {}";
        assert!(run(src).is_empty());
    }
}
