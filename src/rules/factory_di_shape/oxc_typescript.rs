//! factory-di-shape — oxc backend.
//!
//! Flags exported `create*` functions in TypeScript (`.ts`/`.tsx`) that take
//! 3+ separate dependency parameters instead of a single deps object. The
//! dependency-injection smell is multiple *service* dependencies, so only
//! non-primitive (interface/class/type-reference/object/function) params count
//! toward the threshold. Params with primitive types (`string`/`number`/
//! `boolean`/..., and optionals/unions of only those) are configuration
//! values, not injected services, and a `create*` function whose params are
//! all primitives is a value/DOM factory rather than a DI factory.
//!
//! Plain JavaScript is skipped: distinguishing a service dependency from a
//! primitive value relies on parameter type annotations, which JS lacks.

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // The DI-factory heuristic relies on parameter TYPE ANNOTATIONS to tell
        // a service dependency from a primitive value. Plain JavaScript has no
        // param type annotations, so every param looks like a dependency — the
        // heuristic is unreliable there. Restrict to TypeScript (.ts/.tsx).
        if ctx.lang == Language::JavaScript {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            if !trimmed.contains("export") || !trimmed.contains("function create") {
                continue;
            }

            let open = match trimmed.find('(') {
                Some(p) => p,
                None => continue,
            };
            let close = match trimmed[open..].find(')') {
                Some(p) => open + p,
                None => continue,
            };

            let params_str = &trimmed[open + 1..close];
            if params_str.trim().starts_with('{') {
                continue;
            }

            let dep_count = params_str
                .split(',')
                .filter(|p| !p.trim().is_empty())
                .filter(|p| !param_is_primitive(p))
                .count();

            if dep_count >= 3 {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`create*` factory with {dep_count} separate dependency params \u{2014} \
                         use a single deps object: \
                         `createService({{ db, cache, logger }})`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

/// Whether a single param segment (e.g. `nonce?: string`) is typed with only
/// primitive types. An unannotated param is treated as non-primitive: it
/// cannot be proven to be a configuration value.
pub(super) fn param_is_primitive(param: &str) -> bool {
    let annotation = match param.split_once(':') {
        Some((_name, ty)) => ty.trim(),
        None => return false,
    };
    if annotation.is_empty() {
        return false;
    }
    // A union (`string | number`) is primitive iff every member is.
    annotation.split('|').all(|member| type_atom_is_primitive(member.trim()))
}

/// Whether a single type atom (no top-level `|`) is primitive.
fn type_atom_is_primitive(ty: &str) -> bool {
    let ty = ty.trim();
    if ty.is_empty() {
        return false;
    }
    if matches!(
        ty,
        "string"
            | "number"
            | "boolean"
            | "bigint"
            | "symbol"
            | "null"
            | "undefined"
            | "void"
            | "true"
            | "false"
    ) {
        return true;
    }
    // String literal type: `'foo'` or `"foo"`.
    if (ty.starts_with('\'') && ty.ends_with('\'') && ty.len() >= 2)
        || (ty.starts_with('"') && ty.ends_with('"') && ty.len() >= 2)
    {
        return true;
    }
    // Numeric literal type: `42`, `3.14`, `-1`.
    if ty.parse::<f64>().is_ok() {
        return true;
    }
    false
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_on_js(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.js")
    }

    #[test]
    fn flags_create_with_three_service_deps() {
        let src = "export function createService(db: DB, cache: Cache, logger: Logger) {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_create_with_deps_object() {
        let src = "export function createService({ db, cache, logger }: Deps) {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_create_with_two_service_deps() {
        let src = "export function createService(db: DB, logger: Logger) {}";
        assert!(run_on(src).is_empty());
    }

    /// #2257: a DOM/value factory whose params are all primitives is not a
    /// DI factory — grouping config values adds no benefit.
    #[test]
    fn ignores_all_primitive_params() {
        let src =
            "export function createStyleTag(style: string, nonce?: string, suffix?: string): HTMLStyleElement {}";
        assert!(run_on(src).is_empty());
    }

    /// Negative-space guard: a real DI factory with 3 interface-typed deps
    /// is still flagged.
    #[test]
    fn still_flags_real_di_factory() {
        let src =
            "export function createUserService(db: Database, cache: Cache, logger: Logger) {}";
        assert_eq!(run_on(src).len(), 1);
    }

    /// Mixed shape: 1 service + 2 config primitives is not a DI smell.
    #[test]
    fn ignores_one_service_with_primitive_config() {
        let src = "export function createClient(db: Database, retries: number, verbose: boolean) {}";
        assert!(run_on(src).is_empty());
    }

    /// Unions of only primitives count as primitive.
    #[test]
    fn primitive_unions_are_config() {
        let src =
            "export function createBox(a: string | number, b: 'x' | 'y', c: boolean | undefined) {}";
        assert!(run_on(src).is_empty());
    }

    /// #3255: plain JavaScript has no param type annotations, so every param
    /// looks non-primitive and the DI heuristic is meaningless. A code-gen
    /// `create*` util in a `.js` file must not be flagged.
    #[test]
    fn ignores_untyped_javascript_codegen_factory() {
        let src = "export function create_static_module(id, env, disabled) { return id + env + disabled; }";
        assert!(run_on_js(src).is_empty());
    }

    /// Load-bearing: the same source DOES flag at a `.ts` path, proving the
    /// `.js` skip (not some other guard) is what suppresses the FP.
    #[test]
    fn same_untyped_factory_flags_at_ts_path() {
        let src = "export function create_static_module(id, env, disabled) { return id + env + disabled; }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// #3255: SvelteKit's 5-param proxy factory in a `.js` file is not a DI
    /// factory and must not be flagged.
    #[test]
    fn ignores_untyped_javascript_proxy_factory() {
        let src = "export function create_field_proxy(target, get, set, issues, path) { return target; }";
        assert!(run_on_js(src).is_empty());
    }

    #[test]
    fn param_is_primitive_classification() {
        assert!(param_is_primitive("style: string"));
        assert!(param_is_primitive("nonce?: string"));
        assert!(param_is_primitive("count: number"));
        assert!(param_is_primitive("flag: boolean"));
        assert!(param_is_primitive("mode: 'fast' | 'slow'"));
        assert!(param_is_primitive("n: 42"));
        assert!(!param_is_primitive("db: Database"));
        assert!(!param_is_primitive("cb: () => void"));
        assert!(!param_is_primitive("opts: { a: number }"));
        assert!(!param_is_primitive("db")); // unannotated → non-primitive
        assert!(!param_is_primitive("mixed: string | Logger"));
    }
}
