//! data-clumps OXC backend.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::conforms_to_named_function_type;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const FRAMEWORK_CALLBACK_METHODS: &[&str] = &[
    "register", "addHook", "route", "get", "post", "put", "patch", "delete", "head", "options",
    "all",
];

/// Well-known Node.js HTTP middleware signatures (Connect/Express/NestJS), as
/// sorted, deduped parameter-name sets. The framework invokes these callbacks by
/// arity and position, so the group cannot be refactored into a single object
/// parameter — it is a contract, not a data clump.
const FRAMEWORK_MIDDLEWARE_SIGNATURES: &[&[&str]] = &[
    &["next", "req", "res"],          // (req, res, next)
    &["err", "next", "req", "res"],   // (err, req, res, next)
];

/// A 3-param subset is exempt when it is contained in a known middleware
/// signature: every subset of `(req, res, next)` / `(err, req, res, next)` is a
/// framework-mandated group, not a refactorable clump.
fn is_framework_middleware_subset(subset: &[String]) -> bool {
    FRAMEWORK_MIDDLEWARE_SIGNATURES
        .iter()
        .any(|sig| subset.iter().all(|name| sig.contains(&name.as_str())))
}

fn is_test_path(path: &std::path::Path) -> bool {
    let lower = path.to_string_lossy().replace('\\', "/");
    lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains("/fixtures/")
        || lower.contains("/__tests__/")
        || lower.contains(".test.")
        || lower.contains(".spec.")
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
enum FnLocation {
    Local(usize),
    External(std::path::PathBuf, String, usize),
}

/// One function's participation in a shared parameter subset.
#[derive(Clone)]
struct ParamOccurrence {
    location: FnLocation,
    /// The function's total identity-carrying parameter count, compared against
    /// the subset size to tell a strict subset from a saturated signature.
    param_count: usize,
}

/// Extract parameter names from a function's formal parameters.
///
/// Two kinds of parameter are dropped because they carry no semantic identity and
/// cannot be members of a meaningful parameter group:
/// - Underscore-prefixed names (`_`, `__`, `_done`, …): intentionally-unused
///   positional placeholders, ignore markers fixed by the callee's API.
/// - Single-character names that are never read in the function body: arity /
///   positional placeholders (e.g. a generated function's `.length` stamp, whose
///   body reads `arguments` instead). A single-character parameter that IS read
///   (a `(x, y, z)` coordinate) keeps its identity and stays a candidate.
fn extract_param_names(params: &FormalParameters, scoping: &oxc_semantic::Scoping) -> Vec<String> {
    let mut names = Vec::new();
    for param in &params.items {
        if let BindingPattern::BindingIdentifier(ref id) = param.pattern
            && !id.name.starts_with('_')
            && !is_unused_single_char_placeholder(id, scoping)
        {
            names.push(id.name.to_string());
        }
    }
    if let Some(ref rest) = params.rest
        && let BindingPattern::BindingIdentifier(id) = &rest.rest.argument
        && !id.name.starts_with('_')
        && !is_unused_single_char_placeholder(id, scoping)
    {
        names.push(id.name.to_string());
    }
    names
}

/// A single-character parameter that is never read in its function body is an
/// arity/positional placeholder (e.g. a generated function's `.length` stamp,
/// whose body reads `arguments` instead). It carries no semantic identity and
/// cannot be a member of a meaningful parameter group, so it is not a data-clump
/// candidate. A single-character parameter that IS read (a `(x, y, z)` coordinate)
/// keeps its identity and stays a candidate.
fn is_unused_single_char_placeholder(
    id: &BindingIdentifier,
    scoping: &oxc_semantic::Scoping,
) -> bool {
    id.name.chars().count() == 1
        && id.symbol_id.get().is_some_and(|sym| {
            !scoping
                .get_resolved_references(sym)
                .any(|r| r.flags().contains(oxc_semantic::ReferenceFlags::Read))
        })
}

/// Check if a function node is a callback to a framework method like
/// `fastify.register(...)` or a constructor like `new MutationCache({...})`.
fn is_framework_callback<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    // Walk up: function -> ObjectProperty -> ObjectExpression -> CallExpression/NewExpression
    let mut cur = parent_id;
    let mut in_object_expr = false;
    for _ in 0..4 {
        let kind = nodes.kind(cur);
        match kind {
            AstKind::ObjectExpression(_) => {
                in_object_expr = true;
            }
            AstKind::NewExpression(_) => {
                // Constructor calls always impose their callback API on the caller.
                let _ = in_object_expr;
                return true;
            }
            AstKind::CallExpression(call) => {
                let callee_text =
                    &source[call.callee.span().start as usize..call.callee.span().end as usize];
                let method = callee_text.rsplit('.').next().unwrap_or(callee_text);
                return FRAMEWORK_CALLBACK_METHODS.contains(&method);
            }
            _ => {}
        }
        let next = nodes.parent_id(cur);
        if next == cur {
            break;
        }
        cur = next;
    }
    false
}

/// True when the function is an operand of a `||`/`??` `LogicalExpression` whose
/// other operand is a member expression — the `obj.prop || function (...) {}` /
/// `obj.prop ?? function (...) {}` idiom that reads an existing dispatch-table
/// entry and otherwise falls back to this function. The `obj.prop` slot is filled
/// and called by external code, so this fallback conforms to that slot's
/// externally-owned signature and its parameters are not a refactorable clump.
fn is_member_property_default(
    node: &oxc_semantic::AstNode<'_>,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let AstKind::LogicalExpression(logical) = nodes.kind(parent_id) else {
        return false;
    };
    if !matches!(logical.operator, LogicalOperator::Or | LogicalOperator::Coalesce) {
        return false;
    }
    // The function is one operand; the idiom requires the OTHER operand to be the
    // `obj.prop` slot being defaulted.
    let fn_span = node.kind().span();
    let other = if logical.left.span() == fn_span {
        &logical.right
    } else {
        &logical.left
    };
    matches!(
        other,
        Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_)
    )
}

/// True when something outside the function fixes its parameter list: a framework
/// or constructor callback slot, a `||`/`??` default filling a dispatch-table slot,
/// or a named type annotating the function value. The author cannot merge those
/// parameters into one object parameter, so they are not a refactorable clump and
/// the function contributes no parameter group.
fn has_externally_fixed_signature<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    is_framework_callback(node, semantic, source)
        || is_member_property_default(node, semantic)
        || conforms_to_named_function_type(node, semantic)
}

/// Generate all sorted subsets of size `k` from `items`.
fn combinations(items: &[String], k: usize) -> Vec<Vec<String>> {
    let mut result = Vec::new();
    let mut combo = vec![0usize; k];
    fn recurse(
        items: &[String],
        k: usize,
        start: usize,
        combo: &mut Vec<usize>,
        depth: usize,
        result: &mut Vec<Vec<String>>,
    ) {
        if depth == k {
            result.push(combo[..k].iter().map(|&i| items[i].clone()).collect());
            return;
        }
        if start + (k - depth) > items.len() {
            return;
        }
        for i in start..items.len() {
            combo[depth] = i;
            recurse(items, k, i + 1, combo, depth + 1, result);
        }
    }
    recurse(items, k, 0, &mut combo, 0, &mut result);
    result
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir || is_test_path(ctx.path) {
            return Vec::new();
        }

        let nodes = semantic.nodes();
        let mut fn_params: Vec<(FnLocation, Vec<String>)> = Vec::new();

        // Collect function parameter sets from the AST
        for node in nodes.iter() {
            let params = match node.kind() {
                AstKind::Function(func) => {
                    if func.body.is_none() {
                        // TypeScript overload signature — a bodyless declaration of
                        // the same underlying function as its implementation, sharing
                        // params by definition. Only the implementation (which has a
                        // body) contributes to the clump count.
                        continue;
                    }
                    if has_externally_fixed_signature(node, semantic, ctx.source) {
                        continue;
                    }
                    Some(&func.params)
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    if has_externally_fixed_signature(node, semantic, ctx.source) {
                        continue;
                    }
                    Some(&arrow.params)
                }
                _ => None,
            };

            if let Some(params) = params {
                let mut names = extract_param_names(params, semantic.scoping());
                names.sort();
                names.dedup();
                if names.len() >= 3 {
                    let span = node.kind().span();
                    let line = crate::oxc_helpers::byte_offset_to_line_col(
                        ctx.source,
                        span.start as usize,
                    )
                    .0;
                    fn_params.push((FnLocation::Local(line), names));
                }
            }
        }

        // Add exported functions from imported modules (cross-file)
        let index = ctx.project.import_index();
        for imp in index.get_imports(ctx.path) {
            let Some(src_path) = &imp.source_path else {
                continue;
            };
            for export in index.get_exports(src_path) {
                if export.params.len() >= 3 {
                    let mut sorted_params = export.params.clone();
                    sorted_params.sort();
                    sorted_params.dedup();
                    if sorted_params.len() >= 3 {
                        fn_params.push((
                            FnLocation::External(src_path.clone(), export.name.clone(), export.line),
                            sorted_params,
                        ));
                    }
                }
            }
        }

        // For each 3-param subset, count which functions contain it.
        let mut subset_occurrences: FxHashMap<Vec<String>, Vec<ParamOccurrence>> =
            FxHashMap::default();
        for (loc, params) in &fn_params {
            for combo in combinations(params, 3) {
                if is_framework_middleware_subset(&combo) {
                    continue;
                }
                subset_occurrences.entry(combo).or_default().push(ParamOccurrence {
                    location: loc.clone(),
                    param_count: params.len(),
                });
            }
        }

        let mut flagged: FxHashSet<FnLocation> = FxHashSet::default();
        let mut results: Vec<(usize, String)> = Vec::new();

        for (subset, occurrences) in &subset_occurrences {
            if occurrences.len() < 2 {
                continue;
            }
            // A function whose whole parameter list is the subset already is the
            // shared type; when it participates alongside wider signatures there
            // is nothing left to extract, only the unrelated advice to pass the
            // narrower signature's group along.
            let saturating = occurrences
                .iter()
                .filter(|o| o.param_count == subset.len())
                .count();
            if saturating > 0 && saturating < occurrences.len() {
                continue;
            }

            let external_locs: Vec<_> = occurrences
                .iter()
                .filter_map(|o| match &o.location {
                    FnLocation::External(path, name, _) => Some((path, name)),
                    _ => None,
                })
                .collect();

            for occurrence in occurrences {
                if let FnLocation::Local(line) = &occurrence.location
                    && flagged.insert(occurrence.location.clone()) {
                        let msg = if external_locs.is_empty() {
                            format!(
                                "Parameters [{}] appear together in {} functions — extract into a type.",
                                subset.join(", "),
                                occurrences.len(),
                            )
                        } else {
                            let ext_names: Vec<_> = external_locs
                                .iter()
                                .map(|(_, name)| name.as_str())
                                .collect();
                            format!(
                                "Parameters [{}] also used by imported function(s): {} — extract into a shared type.",
                                subset.join(", "),
                                ext_names.join(", "),
                            )
                        };
                        results.push((*line, msg));
                    }
            }
        }

        results.sort_by_key(|(line, _)| *line);
        results
            .into_iter()
            .map(|(line, message)| Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: super::META.id.into(),
                message,
                severity: Severity::Error,
                span: None,
            })
            .collect()
    }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn no_fp_on_object_literal_callbacks_in_new_expression() {
        // Regression for issue #751 — MutationCache constructor callbacks share params by library contract.
        let src = r#"
new MutationCache({
  onError(_e, _variables, _context, _mutation) {},
  onSuccess(_d, _variables, _context, _mutation) {},
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_free_standing_functions_sharing_params() {
        let src = r#"
function createUser(name: string, email: string, age: number) {}
function updateUser(name: string, email: string, age: number) {}
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_on_underscore_prefixed_placeholder_params() {
        // Regression for issue #3857 — `_` and `_done` are intentionally-unused
        // positional placeholders; after dropping them only `body` remains, below
        // the clump threshold, so the pair must not be flagged.
        let src = r#"
function f1(_: unknown, body: string, _done: () => void) {}
function f2(_: unknown, body: string, _done: () => void) {}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_real_params_when_placeholders_dropped() {
        // After dropping `_`, three semantically-named params remain in both
        // functions, so the clump must still flag and the message must list only
        // the real params.
        let src = r#"
function f1(_: unknown, userId: string, name: string, email: string) {}
function f2(_: unknown, userId: string, name: string, email: string) {}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 2);
        assert!(diags[0].message.contains("email, name, userId"));
        assert!(!diags[0].message.contains('_'));
    }

    #[test]
    fn no_fp_on_typescript_overload_signatures() {
        // Regression for issue #6308 — TypeScript overload signatures (bodyless
        // declarations) are not separate functions; they declare the same
        // underlying function as the implementation. The shared parameter triple
        // appears in only one real function (the implementation), so it must not
        // be flagged as a clump.
        let src = r#"
class EventSource {
  public addEventListener(type: string, listener: EventHandler, options?: AddEventListenerOptions): void
  public addEventListener(type: string, listener: EventListener, options?: AddEventListenerOptions): void
  public addEventListener(type: string, listener: EventListenerObject, options?: AddEventListenerOptions): void
  public addEventListener(type: string, listener: EventHandler | EventListener, options?: AddEventListenerOptions): void {
    return;
  }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_distinct_bodied_functions_sharing_params() {
        // Negative control for issue #6308 — three DISTINCT functions, each with a
        // body, sharing the same parameter triple are a genuine clump and must
        // still be flagged. Only bodyless overload signatures are skipped.
        let src = r#"
function createUser(name: string, email: string, age: number) { return; }
function updateUser(name: string, email: string, age: number) { return; }
function deleteUser(name: string, email: string, age: number) { return; }
"#;
        assert_eq!(run(src).len(), 3);
    }

    #[test]
    fn no_fp_on_express_nest_middleware_signature() {
        // Regression for issue #2035 — (req, res, next) is the Node.js HTTP
        // middleware contract, invoked by arity/position, not a data clump.
        let src = r#"
export class AppModule {
  configure(consumer: MiddlewareConsumer) {
    consumer
      .apply((req, res, next) => res.end(MIDDLEWARE_VALUE))
      .forRoutes({ path: MIDDLEWARE_VALUE })
      .apply((req, res, next) => res.status(201).end(MIDDLEWARE_VALUE))
      .forRoutes({ path: MIDDLEWARE_VALUE })
      .apply((req, res, next) => res.end(MIDDLEWARE_PARAM_VALUE))
      .forRoutes({ path: MIDDLEWARE_VALUE });
  }
}
function logger(req, res, next) { next(); }
function auth(req, res, next) { next(); }
function errorHandler(err, req, res, next) { next(err); }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_unused_single_char_arity_placeholders() {
        // Regression for issue #6349 — sinon `src/sinon/proxy.js` generates
        // arity-stamped proxies whose single-char params (`a, b, c, …`) exist only
        // to fix `.length`; the body reads `arguments`, never the params. They
        // carry no semantic identity, so the accidental subset relationship between
        // higher- and lower-arity signatures is not a data clump.
        let src = r#"
function proxy3(a, b, c) { return invoke(this, arguments); }
function proxy4(a, b, c, d) { return invoke(this, arguments); }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_single_char_params_that_are_read() {
        // Single-char params that ARE read (`(x, y, z)` coordinates) keep their
        // identity and remain a refactorable clump — this is why dropping by name
        // length alone would be wrong.
        let src = r#"
function move(x, y, z) { return x + y + z; }
function scale(x, y, z) { return x * y * z; }
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_on_member_property_logical_default() {
        // Regression for issue #7221 — markdown-it's RenderRule signature
        // `(tokens, idx, options, env, self)` is imposed by the library. The
        // `md.renderer.rules.link_open || function (...)` default reads the existing
        // dispatch slot and is exempt; that leaves the shared subset in only the
        // bare `md.renderer.rules.link_open = function (...)` assignment (one
        // function), below the 2-function threshold, so neither line is flagged.
        let src = r#"
const defaultRender = md.renderer.rules.link_open || function (tokens, idx, options, env, self) {
  return self.renderToken(tokens, idx, options)
}
md.renderer.rules.link_open = function (tokens, idx, options, env, self) {
  return defaultRender(tokens, idx, options, env, self)
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_two_member_property_logical_defaults() {
        // Control that specifically exercises the `||`/`??`-member-default arm:
        // both functions default a member-property dispatch slot, so each is exempt
        // and their shared `[alpha, beta, gamma]` subset is never counted.
        let src = r#"
const r1 = obj.handlers.open || function (alpha, beta, gamma) { return alpha; }
const r2 = obj.handlers.close ?? function (alpha, beta, gamma) { return beta; }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_member_assignment_paired_with_standalone() {
        // The bare `obj.prop = function (...)` member-ASSIGNMENT position is NOT
        // exempted — only the `||`/`??` member-default is. Paired with a plain
        // standalone function sharing the triple, the clump must still flag.
        let src = r#"
exports.createUser = function (name, email, age) { return name; }
function updateUser(name, email, age) { return email; }
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_on_method_decorator_factory_returns() {
        // Regression for issue #6858 — nest's message-pattern decorators return the
        // TypeScript `MethodDecorator` callback from four factories. The declared
        // `MethodDecorator` return type fixes the returned function's parameter list,
        // so `(target, key, descriptor)` cannot be merged into an object parameter.
        let src = r#"
export const MessagePattern = (metadata?: unknown): MethodDecorator => {
  return (target: object, key: string | symbol, descriptor: PropertyDescriptor) => {
    return descriptor;
  };
};
export function GrpcMethod(service?: string): MethodDecorator {
  return (target: object, key: string | symbol, descriptor: PropertyDescriptor) => {
    return MessagePattern(service)(target, key, descriptor);
  };
}
export function GrpcStreamMethod(service?: string): MethodDecorator {
  return (target: object, key: string | symbol, descriptor: PropertyDescriptor) => {
    descriptor.value = target;
    return key;
  };
}
export function GrpcStreamCall(service?: string): MethodDecorator {
  return (target: object, key: string | symbol, descriptor: PropertyDescriptor) => {
    return MessagePattern(service)(target, key, descriptor);
  };
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_returned_functions_without_a_declared_return_type() {
        // Minimal delta from the decorator-factory case: with no declared return
        // type, nothing fixes the returned functions' parameter list, so the shared
        // triple is a refactorable clump and must still be flagged.
        let src = r#"
export function makeCreate(service: string) {
  return (target: object, key: string, descriptor: PropertyDescriptor) => descriptor;
}
export function makeUpdate(service: string) {
  return (target: object, key: string, descriptor: PropertyDescriptor) => target;
}
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_on_named_type_annotated_bindings() {
        // A binding annotated with a named type takes that type's parameter list;
        // the group belongs to `RenderRule`, declared elsewhere, not to these two
        // implementations.
        let src = r#"
const renderLink: RenderRule = (tokens, idx, options) => tokens[idx];
const renderImage: RenderRule = (tokens, idx, options) => tokens[options];
"#;
        assert!(run(src).is_empty());
    }

    /// Regression for issue #8298 — Effect-TS's vitest wrapper annotates its
    /// bindings with an indexed access on a named type. `Tester<R>["only"]`
    /// owns that parameter list exactly as a bare `Test<R>` would.
    #[test]
    fn no_fp_on_indexed_access_annotated_bindings_issue_8298() {
        let src = r#"
const only: Vitest.Tester<R>["only"] = (name, self, timeout) => run(name, self, timeout);
const skip: Vitest.Tester<R>["skip"] = (name, self, timeout) => run(name, self, timeout);
"#;
        assert!(run(src).is_empty());
    }

    /// Control: the indexed access reads an inline type literal, whose members
    /// are written at the same site as the functions, so the parameter list is
    /// still the author's to change.
    #[test]
    fn still_flags_inline_indexed_access_annotated_bindings() {
        let src = r#"
const only: { only: (name: string, self: string, timeout: number) => void }["only"] =
  (name, self, timeout) => run(name, self, timeout);
const skip: { skip: (name: string, self: string, timeout: number) => void }["skip"] =
  (name, self, timeout) => run(name, self, timeout);
"#;
        assert_eq!(run(src).len(), 2);
    }

    /// Regression for issue #8298 — a curried factory whose own position is
    /// annotated declares no return type, but the constraint on that position
    /// reaches the function it returns.
    #[test]
    fn no_fp_on_curried_annotated_factories_issue_8298() {
        let src = r#"
const skipIf: Factory = (condition) => (name, self, timeout) => run(name, self, timeout);
const runIf: Factory = (condition) => (name, self, timeout) => run(name, self, timeout);
"#;
        assert!(run(src).is_empty());
    }

    /// The block-body spelling of the same shape: the constraint reaches the
    /// returned function through a `return` statement too.
    #[test]
    fn no_fp_on_curried_annotated_factories_with_block_body() {
        let src = r#"
const skipIf: Factory = (condition) => {
  return (name, self, timeout) => run(name, self, timeout);
};
const runIf: Factory = (condition) => {
  return (name, self, timeout) => run(name, self, timeout);
};
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_named_type_annotated_function_expressions() {
        // The `function (…)` form of an annotated binding takes its parameter list
        // from `RenderRule` as the arrow form does.
        let src = r#"
const open: RenderRule = function (tokens, idx, options) { return tokens[idx]; };
const close: RenderRule = function (tokens, idx, options) { return tokens[options]; };
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_inline_function_type_annotated_bindings() {
        // An inline function type is written at the same site as the function, so
        // its parameter list is the author's to change: the clump stays reportable.
        let src = r#"
const createUser: (name: string, email: string, age: number) => void = (name, email, age) => { return; };
const updateUser: (name: string, email: string, age: number) => void = (name, email, age) => { return; };
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_on_as_annotated_functions() {
        // An `as` assertion pins the named type onto the function value.
        let src = r#"
const open = ((tokens, idx, options) => tokens[idx]) as RenderRule;
const close = ((tokens, idx, options) => tokens[options]) as RenderRule;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_functions_under_a_const_assertion() {
        // `as const` prevents literal-type widening and declares no signature, so
        // the parameter group stays the author's to extract.
        let src = r#"
const open = ((tokens, idx, options) => tokens[idx]) as const;
const close = ((tokens, idx, options) => tokens[options]) as const;
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_on_satisfies_annotated_functions() {
        // A `satisfies` assertion pins the named type onto the function value.
        let src = r#"
const open = ((tokens, idx, options) => tokens[idx]) satisfies RenderRule;
const close = ((tokens, idx, options) => tokens[options]) satisfies RenderRule;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_angle_bracket_assertion_annotated_functions() {
        // `<T>value` is the older spelling of `value as T` and pins the same named
        // type onto the function value.
        let src = r#"
const open = <RenderRule>((tokens, idx, options) => tokens[idx]);
const close = <RenderRule>((tokens, idx, options) => tokens[options]);
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_functions_inside_an_annotated_object_literal() {
        // The annotation types the object, not the functions inside it, so these
        // stay reportable.
        let src = r#"
const handlers: RendererRules = {
  link_open: (tokens, idx, options) => tokens[idx],
  link_close: (tokens, idx, options) => tokens[options],
};
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_on_annotated_class_field_functions() {
        // An annotated class field pins the named type onto the function value.
        let src = r#"
class Renderer {
  fence: RenderRule = (tokens, idx, options) => tokens[idx];
  code: RenderRule = (tokens, idx, options) => tokens[options];
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_curried_decorator_factories() {
        // The curried form of a decorator factory: the returned function is the
        // concise body of an arrow declaring `MethodDecorator`. That declared
        // return type fixes the parameter list, as a `return` statement does.
        let src = r#"
export const GrpcMethod = (service: string): MethodDecorator =>
  (target: object, key: string | symbol, descriptor: PropertyDescriptor) => descriptor;
export const GrpcStreamCall = (service: string): MethodDecorator =>
  (target: object, key: string | symbol, descriptor: PropertyDescriptor) => target;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_curried_factories_without_a_declared_return_type() {
        // Minimal delta from the curried case: with no declared return type the
        // concise body is an ordinary function whose parameters are refactorable.
        let src = r#"
export const makeCreate = (service: string) =>
  (target: object, key: string, descriptor: PropertyDescriptor) => descriptor;
export const makeUpdate = (service: string) =>
  (target: object, key: string, descriptor: PropertyDescriptor) => target;
"#;
        assert_eq!(run(src).len(), 2);
    }

    /// Regression for issue #8157 — the shared triple is `send`'s entire
    /// parameter list, so the "type" to extract is the signature `send` already
    /// declares; there is nothing for either function to factor out.
    #[test]
    fn no_fp_on_clump_saturating_one_signature_issue_8157() {
        let src = r#"
function send(host: string, port: number, proto: string) { return host; }
function connect(host: string, port: number, proto: string, timeout: number) { return port; }
"#;
        assert!(run(src).is_empty());
    }

    /// Control for the saturation check: when the triple is a strict subset of
    /// both parameter lists, extracting it leaves both signatures smaller and
    /// the clump must still be flagged.
    #[test]
    fn still_flags_clump_that_saturates_no_signature() {
        let src = r#"
function send(host: string, port: number, proto: string, retries: number) { return host; }
function connect(host: string, port: number, proto: string, timeout: number) { return port; }
"#;
        assert_eq!(run(src).len(), 2);
    }
}
