//! prefer-global-this OXC backend — flag `window.X` / `self.X` / `global.X`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

/// True if the project's `package.json` declares a browser-like runtime target.
fn project_allows_window(
    project: &crate::project::ProjectCtx,
    path: &std::path::Path,
) -> bool {
    let Some(pkg) = project.nearest_package_json(path) else {
        return false;
    };
    pkg.has_browserslist
        || pkg.engines.contains_key("vscode")
        || pkg.engines.contains_key("electron")
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
        // libraries (Effect-TS, fp-ts, arktype). `is_reference_to_global_variable`
        // is true only when the name resolves to an unbound (global) reference,
        // so a shadowed local returns false and is left alone.
        if !semantic.is_reference_to_global_variable(obj) {
            return;
        }

        // The project allowlist requires a `package.json` lookup (locked,
        // per-directory memoised) — gate it behind the rare identifier match
        // above so the vast majority of `a.b` accesses skip it entirely.
        if project_allows_window(ctx.project, ctx.path) {
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
            severity: Severity::Warning,
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
}
