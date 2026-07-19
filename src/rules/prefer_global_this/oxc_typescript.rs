//! prefer-global-this OXC backend — flag `window.X` / `self.X` / `global.X`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, source_contains};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

/// True when `src` carries a Nuxt-specific marker (auto-import alias, Nuxt
/// composable, or a `defineNuxt*` macro). Used to gate the `.client.` file-name
/// exemption to genuine Nuxt files, so a same-named module in an unrelated
/// project (e.g. a Node-side gRPC `service.client.ts`) stays subject to the rule.
fn is_nuxt_source(src: &str) -> bool {
    source_contains(src, "#imports")
        || source_contains(src, "nuxt/app")
        || source_contains(src, "#app")
        || source_contains(src, "defineNuxtConfig")
        || source_contains(src, "defineNuxtPlugin")
        || source_contains(src, "defineNuxtRouteMiddleware")
        || source_contains(src, "useNuxtApp")
        || source_contains(src, "useRuntimeConfig")
}

/// True when the project targets a browser realm, so a bare `window` is correct
/// by construction rather than a portability oversight. Two signals:
///
/// - the nearest `package.json` declares a browser-like runtime target
///   (`browserslist`, or an `engines.vscode`/`engines.electron` host), or
/// - the file belongs to a bundler-built browser *application* — a bundler
///   (Vite/webpack/…, via the shared [`project_uses_bundler`] lever) plus a
///   root `index.html` app-entry document. Both are required: the bundler alone
///   also describes library-mode packages, which may legitimately target
///   `globalThis`; the `index.html` entry is what marks a browser app whose only
///   realm is the DOM.
///
/// [`project_uses_bundler`]: crate::rules::file_extension_in_import::project_uses_bundler
fn project_allows_window(ctx: &CheckCtx) -> bool {
    if let Some(pkg) = ctx.project.nearest_package_json(ctx.path)
        && (pkg.has_browserslist
            || pkg.engines.contains_key("vscode")
            || pkg.engines.contains_key("electron"))
    {
        return true;
    }

    crate::rules::file_extension_in_import::project_uses_bundler(ctx)
        && ctx.project.package_root_has_index_html(ctx.path)
}

/// Window-specific APIs that should remain as `window.X`.
const WINDOW_SPECIFIC: &[&str] = &[
    "close", "closed", "stop", "focus", "blur", "frames", "length", "top",
    "opener", "parent", "frameElement", "open", "postMessage", "navigation",
    "name", "locationbar", "menubar", "personalbar", "scrollbars", "statusbar",
    "toolbar", "status", "originAgentCluster",
    "screen", "visualViewport", "moveTo", "moveBy", "resizeTo", "resizeBy",
    "innerWidth", "innerHeight", "outerWidth", "outerHeight",
    "scrollX", "pageXOffset", "scrollY", "pageYOffset", "scroll", "scrollTo",
    "scrollBy", "screenX", "screenLeft", "screenY", "screenTop",
    "devicePixelRatio",
    "addEventListener", "removeEventListener", "dispatchEvent",
    "onresize", "onblur", "onfocus", "onload", "onscroll",
    "onbeforeunload", "onmessage", "onpagehide", "onpageshow", "onunload",
];

/// The accessed property name of a computed member (`obj["prop"]`) when the
/// key is a static string literal, used to honour the `window`-specific
/// allowlist for `window["innerWidth"]`. A dynamic key (`obj[expr]`) has no
/// statically known name, so the allowlist cannot apply and `None` is returned.
fn computed_key_name<'a>(member: &'a ComputedMemberExpression<'a>) -> Option<&'a str> {
    match &member.expression {
        Expression::StringLiteral(lit) => Some(lit.value.as_str()),
        _ => None,
    }
}

/// True if `node` is the operand of a `typeof` unary expression.
fn is_under_typeof<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::UnaryExpression(unary) => {
                if unary.operator == UnaryOperator::Typeof {
                    return true;
                }
            }
            // Stop walking up once past member chain.
            AstKind::StaticMemberExpression(_)
            | AstKind::ComputedMemberExpression(_) => continue,
            _ => return false,
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::StaticMemberExpression,
            AstType::ComputedMemberExpression,
        ]
    }

    // The rule only flags `window.`/`self.`/`global.` member access, so a file
    // carrying none of these identifiers can never trigger it.
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["window", "self", "global"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Both `name.prop` (static) and `name["prop"]` (computed) are handled:
        // `object` is the bare global candidate, `prop_text` is the accessed
        // property name (known for static members and string-literal computed
        // keys, `None` for a dynamic computed key), and `span` covers the whole
        // member expression for the diagnostic location.
        let (object, prop_text, span) = match node.kind() {
            AstKind::StaticMemberExpression(member) => (
                &member.object,
                Some(member.property.name.as_str()),
                member.span,
            ),
            AstKind::ComputedMemberExpression(member) => {
                (&member.object, computed_key_name(member), member.span)
            }
            _ => return,
        };

        // Object must be a bare identifier.
        let Expression::Identifier(obj) = object else {
            return;
        };

        let name = obj.name.as_str();
        if !matches!(name, "window" | "self" | "global") {
            return;
        }

        // A local binding named `window`/`self`/`global` shadows the global, so
        // `name.X` is a member access on that local, not on the global object —
        // e.g. the `const self = this` / `const self = { ... }` closure-alias
        // idiom, or `self` used as a receiver parameter in functional-style
        // libraries (Effect-TS, fp-ts, arktype). Resolve THIS reference's binding
        // rather than asking the file-level question "is there a global named
        // `self`?": a resolved symbol means a local declaration (parameter,
        // variable, import) is in scope, so the access is on the local and is
        // left alone. Only a reference with no resolved symbol in any enclosing
        // scope is the true unbound global.
        let Some(ref_id) = obj.reference_id.get() else {
            return;
        };
        if semantic.scoping().get_reference(ref_id).symbol_id().is_some() {
            return;
        }

        // The project allowlist requires a `package.json` lookup (locked,
        // per-directory memoised) — gate it behind the rare identifier match
        // above so the vast majority of `a.b` accesses skip it entirely.
        if project_allows_window(ctx) {
            return;
        }

        // Nuxt strips `*.client.{ts,js}` files entirely from the SSR bundle, so
        // they execute only in the browser and never during server rendering.
        // The `.client.` filename convention is itself the environment guard, so
        // a bare `window`/`self`/`global` there is correct by construction, not a
        // portability oversight. Gated on a Nuxt source marker (the predicate's
        // documented precondition) so an unrelated `*.client.ts` in a non-Nuxt
        // project stays flagged.
        if is_nuxt_source(ctx.source)
            && crate::rules::path_utils::is_nuxt_client_only_file(ctx.path)
        {
            return;
        }

        if name == "window" && prop_text.is_some_and(|p| WINDOW_SPECIFIC.contains(&p)) {
            return;
        }

        // In a Web Worker script `self` is the `DedicatedWorkerGlobalScope` —
        // the canonical, idiomatic global of that realm (there is no `window`).
        // `self.onmessage` / `self.postMessage` are the spec API surface, so
        // rewriting them to `globalThis` obscures intent rather than improving
        // portability. `window`/`global` stay flagged even in worker files.
        if name == "self" && crate::oxc_helpers::is_worker_script(ctx.source) {
            return;
        }

        if is_under_typeof(node, semantic) {
            return;
        }

        // Inside a Playwright/Puppeteer `*.evaluate(...)` callback the code runs
        // in the browser page realm, where `window` is the intended global.
        if crate::oxc_helpers::is_in_browser_eval_callback(node, semantic) {
            return;
        }

        // Inside a named function serialized via `<fn>.toString()` and injected
        // as a script string (Playwright `safeNonStallingEvaluateInAllFrames`,
        // bootstrap polyfills) the body also runs in the browser page realm, so
        // `window` is the intended global there.
        if crate::oxc_helpers::is_inside_tostring_serialized_function(node, semantic) {
            return;
        }

        // A file that feature-detects this global with a `typeof` check
        // (`typeof window !== "undefined"`) is deliberately environment-aware
        // code where the bare alias is the intended object, not a portability
        // oversight — e.g. a browser-only library guarding `window.matchMedia`.
        if crate::oxc_helpers::file_typeof_guards(ctx.source, semantic).guards(name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "prefer-global-this".into(),
            message: format!("Prefer `globalThis` over `{name}`. Replace `{name}.` with `globalThis.`."),
            severity: Severity::Error,
            span: None,
        });
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

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_global_self_access() {
        // Bare global `self.X` with no local binding is still flagged.
        let d = run_ts("self.fetch('/api');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_global_window_access() {
        assert_eq!(run_ts("const url = window.location;").len(), 1);
    }

    #[test]
    fn ignores_self_shadowed_by_local_const() {
        // Regression for #1146: `const self` shadows the global, so member
        // accesses are on the local, not on the browser global.
        let src = "const self: ThisPoller = {\n  \
                   poll: async () => {},\n  \
                   isDone: () => false,\n  \
                   pollUntilDone: () => {\n    \
                   if (!self.isDone()) {\n      \
                   self.poll();\n      \
                   while (!self.isDone()) {\n        \
                   self.poll();\n      \
                   }\n    \
                   }\n  \
                   },\n\
                   };";
        assert!(
            run_ts(src).is_empty(),
            "local `self` binding must not be flagged: {:?}",
            run_ts(src)
        );
    }

    #[test]
    fn ignores_self_aliased_to_this() {
        // The classic `const self = this` closure-alias idiom.
        let src = "function C() {\n  const self = this;\n  return () => self.run();\n}";
        assert!(run_ts(src).is_empty(), "{:?}", run_ts(src));
    }

    #[test]
    fn ignores_window_local_binding() {
        let src = "function f(window: Win) {\n  return window.location;\n}";
        assert!(run_ts(src).is_empty(), "{:?}", run_ts(src));
    }

    #[test]
    fn ignores_self_in_worker_script_with_onmessage() {
        // Regression for #1658: in a Web Worker `self` is the canonical global
        // (`DedicatedWorkerGlobalScope`), so `self.onmessage`/`self.postMessage`
        // must not be rewritten to `globalThis`.
        let src = "self.onmessage = (event) => {\n  \
                   self.postMessage({ msg: 'load worker' })\n\
                   }";
        assert!(run_ts(src).is_empty(), "worker `self` must not be flagged: {:?}", run_ts(src));
    }

    #[test]
    fn ignores_self_post_message_in_worker_script() {
        // The `wasm/worker.js` example from #1658: only `self.postMessage`
        // marks the file as a worker, no `onmessage` handler.
        let src = "import init from './add.wasm?init'\n\
                   init().then(({ exports }) => {\n  \
                   self.postMessage({ result: exports.add(1, 2) })\n\
                   })";
        assert!(run_ts(src).is_empty(), "worker `self` must not be flagged: {:?}", run_ts(src));
    }

    #[test]
    fn flags_self_in_non_worker_file() {
        // Negative-space guard: a file with no worker signals still gets the
        // `self` -> `globalThis` suggestion.
        let d = run_ts("const data = self.crypto.randomUUID();");
        assert_eq!(d.len(), 1, "non-worker `self` must still be flagged: {d:?}");
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_window_even_in_worker_file() {
        // The worker exemption is `self`-only; `window` stays flagged.
        let src = "self.onmessage = () => {};\nconst u = window.location;";
        let d = run_ts(src);
        assert_eq!(d.len(), 1, "`window` must stay flagged in a worker file: {d:?}");
        assert!(d[0].message.contains("window"));
    }

    #[test]
    fn ignores_self_as_function_parameter() {
        // Regression for #1193: in functional TypeScript (Effect-TS, fp-ts,
        // arktype) `self` is the receiver parameter, not the browser global.
        let src = "export const addParam = (name: string) =>\n  \
                   (self: Procedure): Procedure => ({\n    \
                   ...self,\n    \
                   params: { ...self.params, [name]: 1 },\n  \
                   })";
        assert!(
            run_ts(src).is_empty(),
            "`self` bound as a parameter must not be flagged: {:?}",
            run_ts(src)
        );
    }

    #[test]
    fn ignores_self_computed_member_as_parameter() {
        // A `self` parameter accessed via computed member must also be exempt.
        let src = "const get = (self: Rec, key: string) => self[key];";
        assert!(
            run_ts(src).is_empty(),
            "computed access on a local `self` must not be flagged: {:?}",
            run_ts(src)
        );
    }

    #[test]
    fn flags_global_self_computed_member() {
        // Negative-space guard: a genuine global `self` accessed via computed
        // member (no local `self` in scope) is still flagged.
        let d = run_ts("self['location'].reload();");
        assert_eq!(d.len(), 1, "global `self[...]` must still fire: {d:?}");
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn ignores_local_self_param_when_unbound_self_exists_elsewhere() {
        // Regression for #6143 (pinojs/pino browser.js): a `self` member access
        // bound to a function parameter must not be flagged just because an
        // unrelated, unbound `self` reference (a globalThis polyfill fallback)
        // appears elsewhere in the same file. The unbound `self` here is a call
        // argument, not a member access, so it is never examined either.
        let src = "function set(self, opts, rootLogger, level) {\n  \
                   const x = self.level;\n  \
                   return x;\n\
                   }\n\
                   function globalEnv() {\n  \
                   try {\n    \
                   return globalThis;\n  \
                   } catch (e) {\n    \
                   return defd(self) || {};\n  \
                   }\n\
                   }";
        assert!(
            run_ts(src).is_empty(),
            "local `self` param must not be flagged despite an unbound `self` elsewhere: {:?}",
            run_ts(src)
        );
    }

    #[test]
    fn flags_unbound_self_member_only_not_local_self_param() {
        // Per-reference discrimination: in the same file a `self.X` bound to a
        // parameter must stay silent while an unbound `self.X` in another scope
        // still fires. Exactly one diagnostic, on the global access.
        let src = "function set(self) {\n  \
                   return self.level;\n\
                   }\n\
                   function other() {\n  \
                   return self.location;\n\
                   }";
        let d = run_ts(src);
        assert_eq!(
            d.len(),
            1,
            "only the unbound `self.location` must fire, not the bound `self.level`: {d:?}"
        );
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn ignores_window_in_tostring_serialized_function_declaration() {
        // Regression for #5534 (crDragDrop.ts): a named `function` whose body is
        // serialized via `fn.toString()` into a template-literal script and
        // injected into the browser. `window.*` is the correct browser global.
        let src = "function setupDragListeners() {\n  \
                   window.addEventListener('mousemove', l, { once: true });\n  \
                   window.__cleanupDrag = async () => {\n    \
                   delete window.__cleanupDrag;\n  \
                   };\n\
                   }\n\
                   evaluateInAllFrames(`(${setupDragListeners.toString()})()`);";
        assert!(
            run_ts(src).is_empty(),
            "`window.*` inside a `.toString()`-serialized function must not be flagged: {:?}",
            run_ts(src)
        );
    }

    #[test]
    fn ignores_window_in_tostring_serialized_function_returned_string() {
        // Regression for #5534 (wvPage.ts): the serialized script is built in a
        // `return` of a template literal interpolating `fn.toString()`.
        let src = "function polyfill() {\n  \
                   window.PublicKeyCredential ??= {} as any;\n\
                   }\n\
                   function makeScript() {\n  \
                   return `(${polyfill.toString()})();`;\n\
                   }";
        assert!(
            run_ts(src).is_empty(),
            "`window.*` in a returned `.toString()` script must not be flagged: {:?}",
            run_ts(src)
        );
    }

    #[test]
    fn ignores_window_in_tostring_serialized_function_concat() {
        // Regression for #5534 (screenshotter.ts): the script is built with
        // string concatenation `'(' + fn.toString() + ')(...)'`.
        let src = "function inPagePrepare() {\n  \
                   window.__pwCleanupScreenshot = () => {};\n\
                   }\n\
                   const script = '(' + inPagePrepare.toString() + ')(arg)';";
        assert!(
            run_ts(src).is_empty(),
            "`window.*` in a concatenated `.toString()` script must not be flagged: {:?}",
            run_ts(src)
        );
    }

    #[test]
    fn ignores_window_in_tostring_serialized_arrow_const() {
        // The injected function can also be an arrow bound to a `const`.
        let src = "const inject = () => {\n  \
                   window.__pwHook = 1;\n\
                   };\n\
                   page.evaluateOnNewDocument(`(${inject.toString()})()`);";
        assert!(
            run_ts(src).is_empty(),
            "`window.*` in a `.toString()`-serialized arrow const must not be flagged: {:?}",
            run_ts(src)
        );
    }

    #[test]
    fn ignores_window_in_tostring_serialized_function_expression_const() {
        // The injected function can be an anonymous function expression bound to
        // a `const`; the binding name is what carries `.toString()`.
        let src = "const inject = function () {\n  \
                   window.__pwHook = 1;\n\
                   };\n\
                   page.evaluate(`(${inject.toString()})()`);";
        assert!(
            run_ts(src).is_empty(),
            "`window.*` in a `.toString()`-serialized function-expression const must not be flagged: {:?}",
            run_ts(src)
        );
    }

    #[test]
    fn ignores_window_in_outer_when_outer_is_serialized() {
        // Nested case: the serialized function is the OUTER one, and `window.*`
        // sits directly in the outer body. The ancestor walk reaches the outer
        // function and finds its `.toString()` reference.
        let src = "function outer() {\n  \
                   window.__pwHook = 1;\n  \
                   function inner() { return 1; }\n  \
                   return inner();\n\
                   }\n\
                   page.evaluate(`(${outer.toString()})()`);";
        assert!(
            run_ts(src).is_empty(),
            "`window.*` in the outer of a serialized function must not be flagged: {:?}",
            run_ts(src)
        );
    }

    #[test]
    fn flags_window_when_only_inner_function_serialized() {
        // Nested case: `window.*` is in the OUTER body but only the INNER
        // function is `.toString()`-serialized. The outer is never serialized,
        // so its `window.*` must still be flagged.
        let src = "function outer() {\n  \
                   const u = window.location;\n  \
                   function inner() { return 1; }\n  \
                   return `(${inner.toString()})()`;\n\
                   }\n\
                   outer();";
        let d = run_ts(src);
        assert_eq!(
            d.len(),
            1,
            "`window.*` in a non-serialized outer must stay flagged even when an inner is serialized: {d:?}"
        );
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_window_in_function_not_serialized() {
        // Negative-space guard: a plain named function that is never
        // `.toString()`-serialized still gets the `globalThis` suggestion — the
        // exemption is keyed on serialization, not on being inside any function.
        let src = "function setup() {\n  \
                   const u = window.location;\n\
                   }\n\
                   setup();";
        let d = run_ts(src);
        assert_eq!(d.len(), 1, "non-serialized `window.*` must still fire: {d:?}");
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_top_level_window_with_tostring_elsewhere() {
        // A `.toString()` call on an *unrelated* function must not exempt a
        // top-level `window.*` in ordinary application code.
        let src = "function other() {}\n\
                   const tag = other.toString();\n\
                   const url = window.location;";
        let d = run_ts(src);
        assert_eq!(
            d.len(),
            1,
            "top-level `window.*` must stay flagged despite an unrelated `.toString()`: {d:?}"
        );
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn ignores_window_in_nuxt_client_only_file() {
        // Regression for #6539: a Nuxt `*.client.ts` plugin is stripped from the
        // SSR bundle and runs only in the browser, so `window.*` is correct by
        // construction (the `.client.` filename is the environment guard).
        let src = "export default defineNuxtPlugin(() => {\n  \
                   window.localStorage?.setItem('k', 'v');\n  \
                   window.matchMedia('(prefers-color-scheme: dark)');\n\
                   });";
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            src,
            "src/runtime/plugin.client.ts",
        );
        assert!(
            d.is_empty(),
            "`window.*` in a Nuxt `.client.ts` file must not be flagged: {d:?}"
        );
    }

    #[test]
    fn flags_window_in_plain_ts_file_negative_control() {
        // Negative control for #6539: the same `window.*` usage in a plain
        // `.ts` file (no `.client.` infix) still fires.
        let src = "export default defineNuxtPlugin(() => {\n  \
                   window.localStorage?.setItem('k', 'v');\n\
                   });";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "src/runtime/plugin.ts");
        assert_eq!(
            d.len(),
            1,
            "`window.*` in a plain `.ts` file must still be flagged: {d:?}"
        );
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_window_in_non_nuxt_client_file() {
        // Negative control for #6539: a `*.client.ts` file with NO Nuxt marker
        // (e.g. a Node-side gRPC client) is not exempt — the exemption is gated
        // on Nuxt detection, not the filename alone.
        let src = "window.localStorage?.setItem('k', 'v');";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "src/service.client.ts");
        assert_eq!(
            d.len(),
            1,
            "`window.*` in a non-Nuxt `.client.ts` file must still be flagged: {d:?}"
        );
        assert!(d[0].message.contains("globalThis"));
    }

    /// Write a `package.json` (plus any `extra_root_files`, e.g. `index.html`) at
    /// a temporary project root, place `src` at `src/copy.ts` under it, and run
    /// the rule against a real `ProjectCtx` that resolves the manifest and root
    /// files from disk. The `TempDir` is held for the duration of the run.
    fn run_in_project(pkg_json: &str, extra_root_files: &[&str], src: &str) -> Vec<Diagnostic> {
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("package.json"), pkg_json).expect("write package.json");
        for name in extra_root_files {
            fs::write(dir.path().join(name), "").expect("write root file");
        }
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src_path = src_dir.join("copy.ts");
        fs::write(&src_path, src).expect("write source");
        let project = crate::project::ProjectCtx::default();
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            &src_path,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    #[test]
    fn ignores_window_in_vite_spa_with_index_html() {
        // Regression for #7766 (chansee97/nova-admin): a Vite/Vue browser SPA has
        // a `vite` dependency and a root `index.html` app-entry, so `window.*` is
        // the correct global by construction — there is no Node realm.
        let d = run_in_project(
            r#"{"name":"nova-admin","dependencies":{"vite":"^5.0.0"}}"#,
            &["index.html"],
            "window.$message.error('boom');",
        );
        assert!(d.is_empty(), "`window.*` in a Vite SPA must not be flagged: {d:?}");
    }

    #[test]
    fn ignores_window_in_webpack_app_with_index_html() {
        // A webpack browser app is recognised the same way: bundler dependency
        // plus a root `index.html` app-entry.
        let d = run_in_project(
            r#"{"name":"app","dependencies":{"webpack":"^5.0.0"}}"#,
            &["index.html"],
            "const x = window.foo;",
        );
        assert!(d.is_empty(), "`window.*` in a webpack browser app must not be flagged: {d:?}");
    }

    #[test]
    fn flags_window_in_bundler_library_without_index_html() {
        // A bundler dependency alone also describes library-mode packages, which
        // may legitimately target `globalThis`. Without a root `index.html`
        // app-entry the project is not proven browser-only, so `window.*` stays
        // flagged — the app-entry guard is what distinguishes app from library.
        let d = run_in_project(
            r#"{"name":"lib","dependencies":{"vite":"^5.0.0"}}"#,
            &[],
            "const x = window.foo;",
        );
        assert_eq!(d.len(), 1, "library-mode bundler project must still flag `window.*`: {d:?}");
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_window_in_plain_node_project() {
        // Negative control: a plain Node project (no bundler, no browserslist/
        // electron/vscode) has a server realm, so `window.*` must still fire.
        let d = run_in_project(
            r#"{"name":"svc","dependencies":{"express":"^4.0.0"}}"#,
            &[],
            "const x = window.foo;",
        );
        assert_eq!(d.len(), 1, "plain Node project must still flag `window.*`: {d:?}");
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn ignores_window_in_browserslist_project() {
        // The pre-existing browser-target exemption keys off `browserslist`
        // alone, independently of the bundler + `index.html` app-entry signal.
        let d = run_in_project(
            r#"{"name":"web","browserslist":["last 2 versions"]}"#,
            &[],
            "const x = window.foo;",
        );
        assert!(d.is_empty(), "`window.*` in a browserslist project must not be flagged: {d:?}");
    }
}
