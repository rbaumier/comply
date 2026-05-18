use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Check;

/// Return a 4-character lowercase prefix bucket for `name`, so close
/// variants such as `cancelReason` and `cancelledAt` collide on the
/// same bucket (`canc`). Returns the empty string when the name has
/// fewer than 4 leading ASCII alphabetic characters.
fn leading_prefix(name: &str) -> String {
    let bytes = name.as_bytes();
    let mut buf = String::with_capacity(4);
    for &b in bytes.iter().take(4) {
        if !b.is_ascii_alphabetic() {
            return String::new();
        }
        buf.push(b.to_ascii_lowercase() as char);
    }
    if buf.len() < 4 {
        return String::new();
    }
    buf
}

fn collect_optional_prefixes(members: &oxc_allocator::Vec<'_, oxc_ast::ast::TSSignature<'_>>) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for member in members.iter() {
        let oxc_ast::ast::TSSignature::TSPropertySignature(prop) = member else {
            continue;
        };
        if !prop.optional {
            continue;
        }
        // Skip phantom / mutually-exclusive-props patterns where the
        // annotation is `never` — those keys MUST be absent, opposite
        // of an optional state flag. (Regression: #120.)
        if let Some(annot) = &prop.type_annotation
            && matches!(annot.type_annotation, oxc_ast::ast::TSType::TSNeverKeyword(_))
        {
            continue;
        }
        let name = match &prop.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            _ => continue,
        };
        let prefix = leading_prefix(name);
        if prefix.len() < 4 {
            continue;
        }
        *counts.entry(prefix).or_insert(0) += 1;
    }
    counts
}

fn check_optional_clusters(
    counts: HashMap<String, usize>,
    type_name: &str,
    span_start: u32,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut hits: Vec<(&String, &usize)> = counts.iter().filter(|(_, c)| **c >= 2).collect();
    if hits.is_empty() {
        return;
    }
    hits.sort_by(|a, b| b.1.cmp(a.1));
    let (prefix, count) = hits[0];
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "`{type_name}` has {count} optional fields sharing prefix `{prefix}\u{2026}` \u{2014} encode this state with a discriminated union instead."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSInterfaceDeclaration, AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::TSInterfaceDeclaration(iface) => {
                let name = iface.id.name.as_str();
                let counts = collect_optional_prefixes(&iface.body.body);
                check_optional_clusters(counts, name, iface.span.start, ctx, diagnostics);
            }
            AstKind::TSTypeAliasDeclaration(alias) => {
                let oxc_ast::ast::TSType::TSTypeLiteral(lit) = &alias.type_annotation else {
                    return;
                };
                let name = alias.id.name.as_str();
                let counts = collect_optional_prefixes(&lit.members);
                check_optional_clusters(counts, name, alias.span.start, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_two_optional_fields_sharing_prefix() {
        let src = "interface Order { id: string; cancelReason?: string; cancelledAt?: string }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_phantom_never_props() {
        // Regression for #120: `{ page?: never; pageSize?: never; q?: never; sort?: never }`
        // is a mutually-exclusive-props / phantom-key pattern. `?: never`
        // declares the key MUST be absent — opposite of an optional
        // state flag, so the cluster heuristic must skip it.
        let src = "type Phantom = { page?: never; pageSize?: never; q?: never; sort?: never };";
        assert!(run_on(src).is_empty());
    }
}
