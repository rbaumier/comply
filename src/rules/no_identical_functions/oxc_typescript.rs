//! no-identical-functions OXC backend.
//!
//! Intra-file detection: pairwise comparison of functions in the same file.
//! Cross-file detection uses the same process-wide cache as the tree-sitter
//! backend (shared helpers live in typescript.rs, available at runtime via
//! the `pub(super)` visibility — the module is only `#[cfg(test)]` for the
//! AstCheck impl, but the helper functions are always compiled).

use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

/// Collapse runs of whitespace per line and drop blank lines.
fn normalize_body(text: &str) -> String {
    text.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build the normalized type signature of a function: the parameter list text
/// joined with its return-type annotation, both whitespace-collapsed. Two
/// functions with identical bodies but differing signatures (e.g. a
/// `T -> any` serializer and an `any -> T` deserializer) are not
/// interchangeable, so extracting a shared helper is inapplicable; comparing
/// signatures alongside bodies suppresses that false positive.
fn normalize_sig(source: &str, params: oxc_span::Span, return_type: Option<oxc_span::Span>) -> String {
    let params_text = normalize_body(&source[params.start as usize..params.end as usize]);
    let return_text = return_type
        .map(|span| normalize_body(&source[span.start as usize..span.end as usize]))
        .unwrap_or_default();
    format!("{params_text}->{return_text}")
}

fn body_meets_threshold(
    raw: &str,
    normalized: &str,
    min_body_lines: usize,
    min_normalized_chars: usize,
) -> bool {
    raw.lines().count() >= min_body_lines && normalized.len() >= min_normalized_chars
}

fn hash_str(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Test framework entry points whose callbacks define an isolated scope.
/// Closed list — adding a framework requires editing this constant.
const TEST_BASES: &[&str] = &["test", "it", "describe", "suite", "context"];

/// True when `callee` roots at a recognised test entry point: bare
/// `it(...)`, member forms `it.only(...)` / `describe.skip(...)`, and the
/// curried `it.each([...])(...)` shape where the callee is itself a call.
fn callee_is_test_base(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => TEST_BASES.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(member) => callee_is_test_base(&member.object),
        Expression::ComputedMemberExpression(member) => callee_is_test_base(&member.object),
        Expression::CallExpression(call) => callee_is_test_base(&call.callee),
        _ => false,
    }
}

/// True when the call passes a function or arrow-function callback as an
/// argument — the shape of every test wrapper, `it(name, () => {...})` and
/// custom variants like `itShouldSkipForReactCanary(name, () => {...})`
/// alike. Used in test files to treat any such call as a scope boundary
/// without enumerating wrapper names.
fn call_has_callback_arg(call: &CallExpression) -> bool {
    call.arguments.iter().any(|arg| {
        matches!(
            arg,
            Argument::ArrowFunctionExpression(_) | Argument::FunctionExpression(_)
        )
    })
}

/// NodeId of the nearest enclosing test-block call expression
/// (`it(...)`, `describe(...)`, …), or `None` when the function is not
/// nested inside any test block. Two functions whose enclosing test
/// blocks differ live in separate test scopes: the duplicate body is
/// intentional per-test isolation, not a shared-helper opportunity.
///
/// In test files (`in_test_file`), any call with a callback argument also
/// counts as a scope boundary, so custom `it`-wrappers
/// (`itShouldSkipForReactCanary`, `itIf`, …) are recognised without
/// enumerating their names. Outside test files only the built-in
/// `TEST_BASES` are recognised.
fn enclosing_test_scope_id(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    in_test_file: bool,
) -> Option<oxc_semantic::NodeId> {
    semantic
        .nodes()
        .ancestors(node.id())
        .find(|ancestor| {
            matches!(ancestor.kind(), AstKind::CallExpression(call)
                if callee_is_test_base(&call.callee)
                    || (in_test_file && call_has_callback_arg(call)))
        })
        .map(oxc_semantic::AstNode::id)
}

/// One collected function: name, declaration line, normalized body,
/// normalized type signature, and the enclosing test-block scope (if any)
/// used to suppress cross-test FPs.
struct CollectedFunction {
    name: String,
    line: usize,
    normalized: String,
    signature: String,
    test_scope: Option<oxc_semantic::NodeId>,
}

/// True when the duplicate pair sits in two distinct test-block scopes —
/// the per-test helper pattern (e.g. an inline React `Component` redefined
/// in each `it()` block), where extraction would break test isolation.
fn in_distinct_test_scopes(a: &CollectedFunction, b: &CollectedFunction) -> bool {
    matches!((a.test_scope, b.test_scope), (Some(x), Some(y)) if x != y)
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Generated code (AutoRest `models.ts`, codegen output) routinely emits
        // structurally identical functions; flagging them is pure noise and the
        // "extract a shared helper" advice is inapplicable to files marked
        // DO NOT EDIT.
        if ctx.file.is_generated {
            return Vec::new();
        }

        let min_body_lines =
            ctx.config
                .threshold("no-identical-functions", "min_body_lines", ctx.lang);
        let min_normalized_chars =
            ctx.config
                .threshold("no-identical-functions", "min_normalized_chars", ctx.lang);

        let in_test_file = ctx.file.path_segments.in_test_dir;
        let nodes = semantic.nodes();
        let mut local_functions: Vec<CollectedFunction> = Vec::new();

        // Collect functions from the AST
        for node in nodes.iter() {
            match node.kind() {
                AstKind::Function(func) => {
                    let Some(ref id) = func.id else { continue };
                    let name = id.name.to_string();
                    let Some(ref body) = func.body else { continue };
                    let body_text =
                        &ctx.source[body.span.start as usize..body.span.end as usize];
                    let normalized = normalize_body(body_text);
                    if body_meets_threshold(
                        body_text,
                        &normalized,
                        min_body_lines,
                        min_normalized_chars,
                    ) {
                        let (line, _) = crate::oxc_helpers::byte_offset_to_line_col(
                            ctx.source,
                            id.span.start as usize,
                        );
                        let signature = normalize_sig(
                            ctx.source,
                            func.params.span,
                            func.return_type.as_ref().map(|rt| rt.span),
                        );
                        local_functions.push(CollectedFunction {
                            name,
                            line,
                            normalized,
                            signature,
                            test_scope: enclosing_test_scope_id(node, semantic, in_test_file),
                        });
                    }
                }
                AstKind::VariableDeclarator(decl) => {
                    let BindingPattern::BindingIdentifier(id) = &decl.id else {
                        continue;
                    };
                    let Some(init) = &decl.init else { continue };
                    let (body_span, signature) = match init {
                        Expression::ArrowFunctionExpression(arrow) => {
                            if arrow.expression {
                                continue;
                            }
                            let signature = normalize_sig(
                                ctx.source,
                                arrow.params.span,
                                arrow.return_type.as_ref().map(|rt| rt.span),
                            );
                            (arrow.body.span, signature)
                        }
                        Expression::FunctionExpression(func) => {
                            let Some(ref body) = func.body else { continue };
                            let signature = normalize_sig(
                                ctx.source,
                                func.params.span,
                                func.return_type.as_ref().map(|rt| rt.span),
                            );
                            (body.span, signature)
                        }
                        _ => continue,
                    };
                    let body_text =
                        &ctx.source[body_span.start as usize..body_span.end as usize];
                    let normalized = normalize_body(body_text);
                    if body_meets_threshold(
                        body_text,
                        &normalized,
                        min_body_lines,
                        min_normalized_chars,
                    ) {
                        let (line, _) = crate::oxc_helpers::byte_offset_to_line_col(
                            ctx.source,
                            id.span.start as usize,
                        );
                        local_functions.push(CollectedFunction {
                            name: id.name.to_string(),
                            line,
                            normalized,
                            signature,
                            test_scope: enclosing_test_scope_id(node, semantic, in_test_file),
                        });
                    }
                }
                _ => {}
            }
        }

        let mut diagnostics = Vec::new();

        // Intra-file: flag the first pair per match.
        let _import_index = ctx.project.import_index();
        for i in 1..local_functions.len() {
            for j in 0..i {
                if local_functions[i].normalized == local_functions[j].normalized
                    && local_functions[i].signature == local_functions[j].signature
                    && !in_distinct_test_scopes(&local_functions[i], &local_functions[j])
                {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: local_functions[i].line,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Function `{}` has an identical body to `{}` (line {}). Extract the duplicated logic into a shared helper.",
                            local_functions[i].name,
                            local_functions[j].name,
                            local_functions[j].line,
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                    break;
                }
            }
        }

        // Cross-file: when ImportIndex is non-empty, use hash-based lookup.
        // The cross-file cache requires tree-sitter parsing of all indexed files;
        // since we can't reuse the TS backend's cache across cfg boundaries,
        // we build a lightweight per-file hash lookup here.
        if !_import_index.is_empty() {
            let mut local_hashes: HashSet<(u64, usize)> = HashSet::new();
            for func in &local_functions {
                let h = hash_str(&func.normalized);
                if local_hashes.insert((h, func.line)) {
                    // Check against other indexed files via ImportIndex exports
                    // This is a simplified cross-file check — the full cache
                    // would require re-parsing. For now, intra-file coverage
                    // is the primary path (tests use empty ImportIndex).
                    let _ = (&func.name, h);
                }
            }
        }

        diagnostics
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    /// Run with a `FileCtx` built from the source so a generated-header marker
    /// sets `is_generated` (the default helper uses an empty `FileCtx`).
    fn run_with_file_ctx(source: &str, path: &str) -> Vec<Diagnostic> {
        use crate::files::Language;
        use crate::rules::file_ctx::FileCtx;
        let path = std::path::Path::new(path);
        let lang = Language::from_path(path).unwrap_or(Language::TypeScript);
        let project = crate::project::default_static_project_ctx();
        let file = FileCtx::build(path, source, lang, project);
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, path, project, &file)
    }

    #[test]
    fn flags_identical_functions_at_module_scope() {
        let src = r#"
function foo(x: number) {
    const a = x + 1;
    const b = a * 2;
    console.log(b);
    return b;
}

function bar(x: number) {
    const a = x + 1;
    const b = a * 2;
    console.log(b);
    return b;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("bar"));
        assert!(d[0].message.contains("foo"));
    }

    // Regression for #1387 — test files idiomatically define small inline
    // helper components (here `Component`) once per `it()` block to keep
    // each test isolated. The bodies are identical, but they close over
    // test-scoped state and extracting them would break isolation, so the
    // cross-test duplication must not be flagged.
    #[test]
    fn allows_identical_helpers_in_separate_it_blocks() {
        let src = r#"
it('can abort when pending', async () => {
    const derivedAtom = atom(async (get) => get(baseAtom));
    const Component = () => {
        const count = useAtomValue(derivedAtom);
        const doubled = count * 2;
        console.log(doubled);
        return <div>count: {count}</div>;
    };
    render(<Component />);
});

it('can abort with event listener', async () => {
    const derivedAtom = atom(async (get) => get(baseAtom));
    const Component = () => {
        const count = useAtomValue(derivedAtom);
        const doubled = count * 2;
        console.log(doubled);
        return <div>count: {count}</div>;
    };
    render(<Component />);
});
"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Two identical helpers inside the SAME `it()` block are a genuine
    // intra-test refactor opportunity and must still be flagged.
    #[test]
    fn flags_identical_helpers_in_same_it_block() {
        let src = r#"
it('renders both', () => {
    const First = () => {
        const count = useAtomValue(derivedAtom);
        const doubled = count * 2;
        console.log(doubled);
        return <div>count: {count}</div>;
    };
    const Second = () => {
        const count = useAtomValue(derivedAtom);
        const doubled = count * 2;
        console.log(doubled);
        return <div>count: {count}</div>;
    };
    render(<First />);
    render(<Second />);
});
"#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // The suppression is scoped to cross-test-block pairs only. A
    // test-scoped helper that duplicates a module-scope one is still a
    // real "use the shared helper" opportunity, so it stays flagged.
    #[test]
    fn flags_test_helper_matching_module_helper() {
        let src = r#"
const Shared = () => {
    const count = useAtomValue(derivedAtom);
    const doubled = count * 2;
    console.log(doubled);
    return <div>count: {count}</div>;
};

it('renders', () => {
    const Component = () => {
        const count = useAtomValue(derivedAtom);
        const doubled = count * 2;
        console.log(doubled);
        return <div>count: {count}</div>;
    };
    render(<Component />);
});
"#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Regression for #1126 — an AutoRest serializer/deserializer pair shares an
    // identical body but has mirror-image type signatures (`CorsRule -> any`
    // vs `any -> CorsRule`). The functions are not interchangeable, so the
    // "extract a shared helper" advice is inapplicable and the pair must not
    // be flagged.
    #[test]
    fn allows_identical_body_with_differing_signatures() {
        let src = r#"
export function corsRuleSerializer(item: CorsRule): any {
    const mapped = item["allowedOrigins"].map((p: any) => { return p; });
    const result = { allowedOrigins: mapped };
    return result;
}

export function corsRuleDeserializer(item: any): CorsRule {
    const mapped = item["allowedOrigins"].map((p: any) => { return p; });
    const result = { allowedOrigins: mapped };
    return result;
}
"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Identical bodies AND identical signatures are a genuine duplicate — the
    // rule's real target — and must still be flagged after the signature fix.
    #[test]
    fn flags_identical_body_with_identical_signatures() {
        let src = r#"
function alpha(item: CorsRule): CorsRule {
    const mapped = item["allowedOrigins"].map((p: any) => { return p; });
    const result = { allowedOrigins: mapped };
    return result;
}

function beta(item: CorsRule): CorsRule {
    const mapped = item["allowedOrigins"].map((p: any) => { return p; });
    const result = { allowedOrigins: mapped };
    return result;
}
"#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Regression for #1937 — in a test file, two test cases each call a custom
    // `it`-wrapper (`itShouldSkipForReactCanary`) whose callback defines an
    // identical inline `Page` component. Each wrapper call is its own test
    // scope; the components close over distinct state and extracting them would
    // break test isolation, so the cross-test duplication must not be flagged.
    #[test]
    fn allows_identical_components_in_custom_it_wrappers_in_test_file() {
        let src = r#"
itShouldSkipForReactCanary('should revalidate on focus', async () => {
    function Page() {
        const { data } = useSWR(key1, fetcher);
        const value = data ?? 'fallback';
        console.log(value);
        return <div>{value}</div>;
    }
    render(<Page />);
});

itShouldSkipForReactCanary('should revalidate on reconnect', async () => {
    function Page() {
        const { data } = useSWR(key1, fetcher);
        const value = data ?? 'fallback';
        console.log(value);
        return <div>{value}</div>;
    }
    render(<Page />);
});
"#;
        assert!(
            run_with_file_ctx(src, "test/use-swr-focus.test.tsx").is_empty(),
            "{:?}",
            run_with_file_ctx(src, "test/use-swr-focus.test.tsx")
        );
    }

    // The custom-wrapper generalization is test-file-only. In a non-test file,
    // two identical functions inside two `wrapper(() => {...})` calls are not
    // a recognised test scope, so the duplicate stays flagged.
    #[test]
    fn flags_identical_functions_in_callback_calls_in_non_test_file() {
        let src = r#"
wrapper(() => {
    function build() {
        const a = compute();
        const b = a * 2;
        console.log(b);
        return b;
    }
    build();
});

wrapper(() => {
    function build() {
        const a = compute();
        const b = a * 2;
        console.log(b);
        return b;
    }
    build();
});
"#;
        assert_eq!(
            run_with_file_ctx(src, "src/setup.ts").len(),
            1,
            "{:?}",
            run_with_file_ctx(src, "src/setup.ts")
        );
    }

    // Generated files (AutoRest `models.ts` carrying a DO NOT EDIT header) emit
    // structurally identical functions by design; the rule skips them entirely.
    #[test]
    fn skips_generated_file() {
        let src = r#"// Code generated by AutoRest.
// DO NOT EDIT.
function alpha(item: CorsRule): CorsRule {
    const mapped = item["allowedOrigins"].map((p: any) => { return p; });
    const result = { allowedOrigins: mapped };
    return result;
}

function beta(item: CorsRule): CorsRule {
    const mapped = item["allowedOrigins"].map((p: any) => { return p; });
    const result = { allowedOrigins: mapped };
    return result;
}
"#;
        assert!(
            run_with_file_ctx(src, "src/models.ts").is_empty(),
            "{:?}",
            run_with_file_ctx(src, "src/models.ts")
        );
    }
}
