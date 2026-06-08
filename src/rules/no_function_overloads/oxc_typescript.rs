use std::collections::HashMap;

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{Declaration, Function, Statement};
use std::sync::Arc;

pub struct Check;

/// Word-boundary-aware search for `needle` within `source[start..end]`.
fn source_contains_ident(source: &str, start: u32, end: u32, needle: &str) -> bool {
    let Some(slice) = source.get(start as usize..end as usize) else { return false; };
    let mut search_from = 0;
    while let Some(pos) = slice[search_from..].find(needle) {
        let abs = search_from + pos;
        let before_ok = abs == 0
            || !slice.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && slice.as_bytes()[abs - 1] != b'_';
        let after_pos = abs + needle.len();
        let after_ok = after_pos >= slice.len()
            || !slice.as_bytes()[after_pos].is_ascii_alphanumeric()
                && slice.as_bytes()[after_pos] != b'_';
        if before_ok && after_ok {
            return true;
        }
        search_from = abs + 1;
    }
    false
}

/// Generic parameter names that appear in this overload signature's return type.
/// Returns `None` when the signature has no generics or no return-type annotation
/// (i.e. cannot be load-bearing for return-type inference).
fn generics_in_return_type(source: &str, f: &Function) -> Option<Vec<String>> {
    let type_params = f.type_parameters.as_deref()?;
    let return_type = f.return_type.as_ref()?;
    let mut names = Vec::new();
    for tp in &type_params.params {
        let name = tp.name.name.as_str();
        if source_contains_ident(source, return_type.span.start, return_type.span.end, name) {
            names.push(name.to_string());
        }
    }
    if names.is_empty() { None } else { Some(names) }
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::Program(program) = node.kind() {
                let mut groups: HashMap<String, Vec<OverloadSig>> = HashMap::new();
                for stmt in &program.body {
                    if let Some(sig) = extract_overload_sig(ctx.source, stmt) {
                        groups.entry(sig.name.clone()).or_default().push(sig);
                    }
                }
                for (name, sigs) in groups {
                    if sigs.len() < 2 {
                        continue;
                    }
                    if preserves_generic_return_inference(&sigs) {
                        continue;
                    }
                    for sig in sigs {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, sig.span_start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "Function '{name}' has overload signatures — overloads \
                                 don't constrain the implementation and break inference. \
                                 Use a union parameter type or a generic signature instead."
                            ),
                            severity: super::META.severity,
                            span: None,
                        });
                    }
                }
            }
        }
        diagnostics
    }
}

struct OverloadSig {
    name: String,
    span_start: u32,
    /// Generic parameter names that appear in this signature's return type.
    /// Empty when the signature has no generics referenced in its return type.
    generics_in_return: Vec<String>,
}

/// True when ALL overload signatures share at least one generic type parameter
/// name that appears in their return type. This signals "overloads load-bearing
/// for generic return-type inference" — collapsing them would widen the return
/// type via union and lose narrow inference at call sites.
fn preserves_generic_return_inference(sigs: &[OverloadSig]) -> bool {
    let mut iter = sigs.iter();
    let Some(first) = iter.next() else {
        return false;
    };
    if first.generics_in_return.is_empty() {
        return false;
    }
    let mut intersection: Vec<String> = first.generics_in_return.clone();
    for sig in iter {
        intersection.retain(|n| sig.generics_in_return.iter().any(|m| m == n));
        if intersection.is_empty() {
            return false;
        }
    }
    true
}

/// Extract overload signature info if `stmt` is a function declaration without a body.
fn extract_overload_sig(source: &str, stmt: &Statement) -> Option<OverloadSig> {
    match stmt {
        Statement::FunctionDeclaration(f) => sig_from_function(source, f),
        Statement::ExportNamedDeclaration(exp) => {
            if let Some(ref decl) = exp.declaration
                && let Declaration::FunctionDeclaration(f) = decl
            {
                return sig_from_function(source, f);
            }
            None
        }
        _ => None,
    }
}

fn sig_from_function(source: &str, f: &Function) -> Option<OverloadSig> {
    if f.body.is_some() {
        return None;
    }
    let name = f.id.as_ref()?.name.to_string();
    let generics_in_return = generics_in_return_type(source, f).unwrap_or_default();
    Some(OverloadSig {
        name,
        span_start: f.span.start,
        generics_in_return,
    })
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_overloaded_function() {
        let source = "
function foo(x: number): string;
function foo(x: string): number;
function foo(x: number | string): string | number { return x as any; }
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn allows_single_signature() {
        assert!(run_on("function foo(x: number): string { return String(x); }").is_empty());
    }

    #[test]
    fn allows_distinct_functions() {
        let source = "function foo(): void {} function bar(): void {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_overloads_load_bearing_for_generic_return_inference() {
        // Regression for #109: overloads of `make<...>(opts)` whose generic `C`
        // appears in each return type are load-bearing — collapsing them would
        // widen `sort` to `unknown` via union.
        let source = r#"
type SortColumns = readonly [string, ...string[]];
type SortFor<T extends SortColumns> = `${T[number]}:asc` | `${T[number]}:desc`;
export function make<const C extends SortColumns>(opts: {
  sortColumns: C; defaultSort: SortFor<C>;
}): { sort: SortFor<C> };
export function make<
  F extends Record<string, unknown>,
  const C extends SortColumns,
>(opts: { filters: F; sortColumns: C; defaultSort: SortFor<C> }): { sort: SortFor<C>; filters: F };
export function make<
  F extends Record<string, unknown>,
  const C extends SortColumns,
>(opts: { filters?: F; sortColumns: C; defaultSort: SortFor<C> }) {
  return {} as any;
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_overloads_when_generics_dont_reach_return_type() {
        // Generics exist but only appear in params, not return — not load-bearing.
        let source = "
function foo<T>(x: T): string;
function foo<T>(x: T, y: number): string;
function foo<T>(x: T, y?: number): string { return ''; }
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn flags_overloads_when_only_some_share_return_generic() {
        // One overload uses T in return, the other doesn't — intersection empty,
        // so the exception does not apply.
        let source = "
function foo<T>(x: T): T;
function foo<T>(x: T): string;
function foo<T>(x: T): any { return x; }
";
        assert_eq!(run_on(source).len(), 2);
    }
}
