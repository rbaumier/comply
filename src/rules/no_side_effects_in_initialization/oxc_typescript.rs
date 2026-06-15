//! no-side-effects-in-initialization OxcCheck backend — flag module-level
//! expression statements whose expression is a call or `new` expression.
//!
//! Exemptions:
//! - demonstration scripts under a relaxed directory (`examples/`, `example/`,
//!   `demo/`, `demos/`, `samples/`, …): the rule is gated off there via
//!   `skip_in_relaxed_dir`, since such files are run directly to show library
//!   usage and are never imported as library modules, so the tree-shaking
//!   concern does not apply;
//! - test files (path heuristic);
//! - test-runner setup files matched by convention path/name (`*.setup.*`,
//!   `setup.*`, `setup-*`, `*-setup`, `globalSetup`, `setupTests.*`, anything
//!   under `test-helpers/`, any Cypress support file under `cypress/support/`,
//!   or a Playwright component-testing setup file at `playwright/index.{ts,js}`),
//!   or by content shape where every top-level call is a standard test-runner
//!   lifecycle hook (`beforeAll`/`beforeEach`/`afterEach`/`afterAll`/
//!   `expect.extend`) whose name is not a local value binding — covering both
//!   Jest setup files (hooks injected as globals, no import) and Vitest setup
//!   files (hooks imported from `"vitest"`);
//! - Vitest benchmark files, matched two ways: by the `.bench.{ts,tsx,js,jsx}`
//!   extension (these are benchmark entry files run directly by `vitest bench`,
//!   never imported or tree-shaken), or by content shape — a module that imports
//!   from `"vitest"` and whose top-level call statements are all `describe(...)` /
//!   `bench(...)` registrations (or standard lifecycle hooks), with a
//!   locally-bound `describe`/`bench` name keeping the module flagged;
//! - vanilla-extract style modules by content shape: any module importing from a
//!   `@vanilla-extract/*` package (`@vanilla-extract/css`,
//!   `@vanilla-extract/recipes`, …). vanilla-extract is a zero-runtime
//!   CSS-in-TypeScript library — files using its API (`globalStyle(...)`,
//!   `style(...)`, `keyframes(...)`, `recipe(...)`, …) are exclusively processed
//!   at compile time by the vanilla-extract build plugin into static CSS and are
//!   never bundled into runtime JavaScript, so the top-level style calls are the
//!   file's whole purpose, not a tree-shaking hazard. The `@vanilla-extract/*`
//!   import is the definitive signal, so an arbitrary `.css.ts` module that does
//!   not import the vanilla-extract API is still flagged;
//! - CLI entry points: files whose name is `bin.{ts,mts,js,mjs}` (the Node.js
//!   `package.json` `"bin"` convention) or any file starting with a `#!`
//!   shebang. Such files are executed directly (`tsx ./bin.ts`, `node ./bin.js`),
//!   never imported as a library, so their top-level CLI bootstrap
//!   (`yargs(hideBin(process.argv)).parse()`, `process.stdin.pipe(...)`, …) is
//!   intentional and not tree-shakeable;
//! - benchmark/profiling harness scripts: files whose stem starts with
//!   `profile-` or `bench-` (`profile-pipeline.ts`, `bench-insert.mjs`), or
//!   those under a `bench/`, `benchmarks/`, or `profiling/` directory. These are
//!   run directly (`bun src/native/profile-pipeline.ts`), never imported, so
//!   their body of top-level `bench(...)` / `console.log(...)` calls is the
//!   harness's intended payload. A `profile-*`/`bench-*` *prefix* is required, so
//!   an ordinary `profileService.ts` library module is still flagged;
//! - server application entry points by content shape: any module with a
//!   `listen(...)` / `*.listen(...)` call either at the top level or inside the
//!   consequent/alternate of a top-level `if` statement (the socket-activation
//!   pattern, where the listen target is chosen between a socket fd and a port).
//!   Fastify/Express/Node HTTP servers start the server this way, so the
//!   surrounding route/hook/middleware/signal-handler registrations are mandatory
//!   side effects, not library code;
//! - Node.js CLI script entry points by content shape: a module that defines a
//!   `main` function at module scope (`async function main() { … }` /
//!   `const main = …`) and invokes it at the top level (`main()` or the
//!   `main().catch(err => { … })` form that surfaces async failures). Such a file
//!   is executed directly (`tsx ./index.mts`, `node ./cli.js`), never imported as
//!   a library module, so its top-level entry invocation is the program's
//!   purpose, not a tree-shakeable side effect. Both the local `main` definition
//!   and the top-level call are required, so a `main()` whose callee is imported,
//!   or a bare unrelated top-level call, is still flagged;
//! - React application entry points by content shape: any module with a
//!   top-level React DOM bootstrap call — `createRoot(...).render(...)`,
//!   `ReactDOM.createRoot(...).render(...)`, `hydrateRoot(...)`, or legacy
//!   `ReactDOM.render(...)` (mounting the app at module level is the entry
//!   file's purpose, and entry points are never imported by other modules);
//! - Solid.js application entry points by content shape: a module that imports
//!   the `render` binding from `"solid-js/web"` and calls it at the top level
//!   (`render(() => <App />, root)`). `render` is Solid.js's app bootstrap, so
//!   mounting the app at module level is the entry file's purpose and the
//!   top-level call is an intentional side effect. The `solid-js/web` import is
//!   required so an ordinary module that happens to call a local `render()` is
//!   still flagged;
//! - Vue 3 application entry points by content shape: a module that creates the
//!   app at module scope with a top-level `createApp(...)` call and mounts it
//!   with a top-level `.mount(...)` call, covering both the split form
//!   (`const app = createApp(App); app.use(router); app.mount('#app')`) and the
//!   chained form (`createApp(App).use(router).mount('#app')`). Mounting the app
//!   at module level is the entry file's purpose, and entry points are never
//!   imported by other modules, so the surrounding `app.use` / `app.component` /
//!   `app.provide` registrations are intentional side effects. Both `createApp`
//!   and `.mount` are required, so an ordinary module that merely calls a local
//!   `createApp()` helper or some unrelated `.mount()` is still flagged;
//! - Preact application entry points by content shape: a module that imports the
//!   `render` binding from `"preact"` and calls it at the top level
//!   (`render(<App />, document.getElementById('app')!)`). `render` is Preact's
//!   app bootstrap, so mounting the app at module level is the entry file's
//!   purpose and the top-level call is an intentional side effect. The `preact`
//!   import is required so an ordinary module that happens to call a local
//!   `render()` is still flagged;
//! - Angular standalone application entry points by content shape: a module that
//!   imports `bootstrapApplication` from `"@angular/platform-browser"` and calls
//!   it at the top level (`bootstrapApplication(AppComponent, appConfig)`,
//!   optionally chained with `.catch(...)`/`.then(...)` to surface bootstrap
//!   failures). `bootstrapApplication` is Angular's standalone app bootstrap, so
//!   bootstrapping the app at module level is the entry file's purpose and the
//!   top-level call is an intentional side effect. The `@angular/platform-browser`
//!   import is required so an ordinary module that happens to call a local
//!   `bootstrapApplication()` is still flagged;
//! - Gulp task-registration files by content shape: a module that imports
//!   `gulp` and registers tasks at the top level (`task(...)`, `gulp.task(...)`,
//!   `series(...)`, `parallel(...)`, …). The registrations are the file's whole
//!   purpose — Gulp runs them by importing the build script — so they are
//!   intentional side effects, not tree-shakeable library code;
//! - Storybook addon manager entry files by content shape: a module that
//!   imports the `addons` API from a Storybook manager package
//!   (`storybook/manager-api`, `@storybook/manager-api`, `@storybook/addons`,
//!   `@storybook/manager`) and registers the addon at the top level
//!   (`addons.register(...)`, `addons.add(...)`, `addons.setConfig(...)`). The
//!   Storybook manager bundle loads these entry files to run the registrations,
//!   so the top-level calls are intentional side effects, not tree-shakeable
//!   library code;
//! - MCP server modules by content shape: a module that imports from
//!   `@modelcontextprotocol/sdk` and registers a request handler at the top level
//!   (`server.setRequestHandler(Schema, handler)`). The MCP SDK requires handlers
//!   to be registered at module scope on the exported server instance (consumed
//!   by the transport layer), so the registrations are intentional initialization
//!   of the exported server's API, not tree-shakeable library code. The MCP SDK
//!   import is required so a stray `.setRequestHandler` on an unrelated object in
//!   a non-MCP module is still flagged;
//! - code-generation utility scripts by content shape: a module that imports a
//!   Node filesystem module (`fs`, `node:fs`, `fs/promises`, …) and whose every
//!   top-level expression statement is a bare-identifier call to the *same*
//!   function, invoked at least twice (`run(arSA, 'ar-SA'); run(beBY, 'be-BY');
//!   …`). Such a file iterates one processing function over a dataset to write
//!   output files — it is executed directly (`tsx generate.ts`), never imported
//!   as a library, so the repeated top-level calls are its intentional payload.
//!   A single top-level call, heterogeneous callees, or no filesystem import are
//!   all still flagged;
//! - library entry barrels by content shape: a module with at least one
//!   `export *` re-export (`export * from "./schemas.js"`) where those star
//!   re-exports outnumber its top-level effectful call/`new` statements. Such a
//!   module surfaces the package's full API through one subpath export — it is
//!   imported to obtain that API, never as a tree-shaking target — so the small
//!   number of accompanying top-level registration calls (e.g. zod's
//!   `config(en())` registering the default English locale at module load) are
//!   intentional initialization. A module with real logic that is not dominated
//!   by `export *` re-exports, or one with no `export *` re-export at all, is
//!   still flagged;
//! - static browser assets under a `public/`, `static/`, or `assets/`
//!   directory: vanilla `<script>`-loaded scripts served verbatim by the web
//!   server, never bundled, so the tree-shaking concern does not apply (a
//!   bundler never processes these files);
//! - package-root script entry files reported by
//!   `ProjectCtx::is_script_entry_file`: a file the nearest `package.json`'s
//!   `scripts` invoke directly (e.g. `"build": "tsx ./build.ts"` makes the
//!   sibling `build.ts` a script entry). Such a file is run as a one-shot
//!   executable by a runner, never `import`-ed by another module and never part
//!   of the published `dist/`, so its top-level build steps are intentional and
//!   the tree-shaking concern does not apply. An ordinary library module the
//!   scripts never invoke is still flagged;
//! - files under a top-level CLI/automation entry directory reported by
//!   `is_top_level_script_dir_path` (`scripts/`, `bin/`, `tools/`, `examples/`,
//!   `example-apps/`, `benchmark/`, `benchmarks/` at the project root). These
//!   are build tools and automation scripts run directly by Node or a build
//!   runner, never `import`-ed as library modules, so their top-level inline
//!   execution is the entry point's purpose. A nested `src/scripts/` library
//!   module is still flagged;
//! - framework entry points reported by `is_framework_entry_point`;
//! - TanStack Start entry files (`app/{client,router,server}.{ts,tsx}` or
//!   `src/app/…`) when the `tanstack-router` framework is detected;
//! - `startTransition(...)` calls whose callee resolves to an import from
//!   `"react"` (React 18 top-level hydration pattern);
//! - Node.js process signal-handler registration: a top-level
//!   `process.on("SIG...", handler)` call (`process.on("SIGINT", …)`,
//!   `process.on("SIGTERM", …)`). CLI entry points register these so the program
//!   shuts down gracefully on interruption — the registration is the program's
//!   intended setup, not a tree-shakeable side effect. The receiver must be the
//!   bare `process` global, the method `on`, and the event a `SIG`-prefixed
//!   string literal, so a `.on(...)` subscription on any other emitter or for a
//!   non-signal event is still flagged;
//! - library plugin registration: a top-level member call `recv.<name>(...)`
//!   whose method is one of a narrow allowlist of unambiguous plugin-registration
//!   idioms — `extend` (dayjs's `dayjs.extend(plugin)`) or `registerPlugin`
//!   (gsap/FilePond's `gsap.registerPlugin(x)`). These are the documented,
//!   required one-time configuration to enable a library feature: idempotent,
//!   I/O-free, mutating only the library's own singleton, so they are intentional
//!   module configuration, not a tree-shaking hazard. The allowlist is kept
//!   deliberately narrow — common method names like `use`, `register`, and `add`
//!   are excluded because they overwhelmingly denote genuine side effects
//!   (`app.use(mw)`, arbitrary `x.register(...)`) and would over-exempt — so a
//!   non-registration member call (`analytics.track(...)`, `db.connect()`,
//!   `app.use(mw)`) is still flagged;
//! - commander.js subcommand-registration builder chains: a top-level fluent
//!   method chain that both registers a subcommand with `.command(...)` and
//!   attaches its handler with `.action(...)`
//!   (`mcp.command("init").description(...).option(...).action(handler)`).
//!   commander requires subcommands to be assembled at module scope on a
//!   `Command` instance, so the chain is the program's intended setup observed
//!   only through the configured command object, not a tree-shakeable side
//!   effect. Requiring both `.command` and `.action` keeps the shape precise: an
//!   unrelated fluent chain with only one of them is still flagged;
//! - data-initialization `forEach`: a top-level
//!   `localArray.forEach(item => localLookup.set(...))` whose receiver is a
//!   module-scoped `const` and whose callback only populates another
//!   module-scoped `const` lookup in place (`map.set`, `set.add`, `obj[k] = v`)
//!   with pure values — a deterministic local data build, not an external
//!   side effect;
//! - builder/fluent configuration: a top-level `obj.method(...)` call whose
//!   receiver `obj` is a bare identifier bound by a same-scope
//!   `const obj = new ...()` declaration. The mutation targets a freshly-built
//!   module-local object that has not escaped, so it is no external side effect
//!   and no tree-shaking hazard. A method call on anything else (an imported
//!   singleton, a `const x = getThing()` whose init is a plain call) is still
//!   flagged;
//! - exports augmentation: a top-level `Object.assign(target, ...)` call whose
//!   `target` is the module's own export object — a bare identifier this module
//!   re-exports (`export default styled` / `export { styled }`) or the CommonJS
//!   `exports` / `module.exports` object. Attaching a secondary namespace to the
//!   module's own export before any consumer observes it (e.g.
//!   `Object.assign(styled, secondary)`, React's `React.useState = useState`)
//!   initializes that export, not external state, so it is no tree-shaking
//!   hazard. `Object.assign` onto an imported singleton, a global (`window`,
//!   `globalThis`), or any non-exported binding is still flagged;
//! - mixin-builder calls: a top-level `f(Exported)` call whose callee is a bare
//!   identifier and at least one argument is a bare identifier the module
//!   exports (`initMixin(Vue)` in a module ending `export default Vue`).
//!   Pre-class-syntax frameworks (Vue 2) assemble a constructor at module scope
//!   by passing it through a chain of such builders that each attach methods to
//!   its prototype — the whole point of importing the module is to obtain the
//!   fully assembled export, so the call is intentional initialization. A bare
//!   `init()` or `register(plugin)` whose argument is not an exported binding is
//!   still flagged;
//! - prototype-patcher `forEach`: a top-level
//!   `localArray.forEach(item => def(Exported, item, …))` whose receiver is a
//!   module-local `const` array and whose callback only patches a single
//!   exported binding — a builder call passing the exported object as its first
//!   argument (`def(arrayMethods, …)`) or a member assignment onto the exported
//!   object (`Exported[k] = v`). Local staging declarations in the body are
//!   ignored. This is the pre-class-syntax pattern for intercepting prototype
//!   methods on an exported object. A `forEach` touching any non-exported
//!   target, or whose receiver is not a module-local const, is still flagged;
//! - Vue reactivity setup: a top-level bare-identifier call to a Vue Composition
//!   API / VueUse reactivity primitive — `watch`, `watchEffect`,
//!   `watchPostEffect`, `watchSyncEffect`, `debouncedWatch`, `watchDebounced`,
//!   `watchThrottled`, `watchOnce` — whose callee is imported from a Vue
//!   ecosystem package (`vue`, `@vue/*`, `@vueuse/core`). Vue composable modules
//!   wire reactive state together with such calls at module scope; the call must
//!   run when the composable is first imported, so it is declarative reactive
//!   registration, not a tree-shaking hazard. The import-source gate keeps a
//!   same-named local `watch()` (or one imported from `node:fs`) flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::path_utils::{
    is_browser_asset_dir_path, is_config_file, is_framework_entry_point,
    is_top_level_script_dir_path,
};
use oxc_ast::ast::{
    Argument, AssignmentTarget, BindingPattern, Declaration, ExportDefaultDeclarationKind,
    Expression, ImportDeclarationSpecifier, Program, Statement,
};
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    [".test.", ".test-d.", ".spec.", "_spec.", ".unit.", "__tests__", "_test.", ".e2e.", ".cy.", ".mock.", ".bench."]
        .iter()
        .any(|m| s.contains(m))
        || s.contains("/dtslint/")
        || s.starts_with("dtslint/")
        || s.contains("/test-d/")
        || s.starts_with("test-d/")
}

/// Test-runner setup files (Vitest `setupFiles`/`globalSetup`, Jest
/// `setupFilesAfterEnv`, …) run their top-level side effects *by contract* —
/// the runner imports them precisely to mutate `process.env`, provision a
/// database, or install matchers. They are never tree-shaken. Matched by
/// convention path/name so a regular module that merely has "setup" inside a
/// longer identifier (e.g. `setupRouter.ts`) is still flagged.
fn is_test_setup_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    if s.contains("/test-helpers/") || s.starts_with("test-helpers/") {
        return true;
    }
    // Cypress support files (`cypress/support/component.ts`, `…/e2e.ts`,
    // `…/commands.ts`) are loaded by the test runner before each run for their
    // top-level registrations — that is the Cypress support-file contract.
    if s.contains("/cypress/support/") || s.starts_with("cypress/support/") {
        return true;
    }
    // Playwright component-testing setup files (`playwright/index.{ts,js}`) are
    // the Playwright analogue of Cypress support files: the runner loads them
    // for their top-level fixture registrations (e.g. `setProjectAnnotations`).
    if s.ends_with("/playwright/index.ts")
        || s.ends_with("/playwright/index.js")
        || s.starts_with("playwright/index.ts")
        || s.starts_with("playwright/index.js")
    {
        return true;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // `*.setup.*` — vitest.setup.ts, jest.setup.ts, app.setup.ts, …
    if name.contains(".setup.") {
        return true;
    }
    let stem = name.split('.').next().unwrap_or("");
    stem == "setup"
        || stem.starts_with("setup-")
        || stem.ends_with("-setup")
        || stem == "setuptests"
        || stem == "globalsetup"
        || stem == "global-setup"
}

/// CLI entry points are executed directly, never imported as a library, so
/// their top-level bootstrap is intentional and not tree-shakeable. Two
/// unambiguous signals mark such a file:
/// - a `#!` shebang (a directly-executed script); or
/// - the `bin.{ts,mts,js,mjs}` filename — the Node.js `package.json` `"bin"`
///   convention. The stem must be exactly `bin`, so an ordinary `binary.ts`
///   module is still flagged.
fn is_cli_entry(path: &std::path::Path, source: &str) -> bool {
    if source.starts_with("#!") {
        return true;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    matches!(name, "bin.ts" | "bin.mts" | "bin.js" | "bin.mjs")
}

/// Benchmark and profiling harness scripts are standalone executables run
/// directly (`bun src/native/profile-pipeline.ts`), never imported as a
/// library, so their body of top-level `bench(...)` / `console.log(...)` calls
/// is the harness's intended payload, not a tree-shaking hazard. Two
/// unambiguous signals mark such a file:
/// - a `profile-*` / `bench-*` filename stem (`profile-pipeline.ts`,
///   `bench-insert.mjs`); the stem must *start* with the prefix, so an ordinary
///   `profileService.ts` library module is still flagged;
/// - a `bench/`, `benchmarks/`, or `profiling/` parent directory segment.
fn is_benchmark_or_profile_script(path: &std::path::Path) -> bool {
    let stem = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .split('.')
        .next()
        .unwrap_or("");
    if stem.starts_with("profile-") || stem.starts_with("bench-") {
        return true;
    }
    path.components().any(|c| {
        matches!(c, std::path::Component::Normal(s)
            if matches!(s.to_str(), Some("bench" | "benchmarks" | "profiling")))
    })
}

const TEST_RUNNER_HOOK_IDENTS: &[&str] =
    &["beforeAll", "beforeEach", "afterEach", "afterAll"];

/// Root identifier of a hook call's callee: the bare name for `beforeEach(...)`,
/// or the object name for the `expect.extend(...)` member call. `None` for any
/// other callee shape.
fn hook_callee_root<'a>(call: &'a oxc_ast::ast::CallExpression) -> Option<&'a str> {
    match &call.callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => {
            let Expression::Identifier(obj) = &m.object else {
                return None;
            };
            if obj.name == "expect" && m.property.name == "extend" {
                Some("expect")
            } else {
                None
            }
        }
        _ => None,
    }
}

/// True when `call` invokes a standard test-runner lifecycle hook
/// (`beforeAll`/`beforeEach`/`afterEach`/`afterAll`/`expect.extend`) whose root
/// identifier is NOT bound by a top-level value declaration in `locals`. An
/// injected runner global (Jest) or a hook imported from the runner package
/// (Vitest) is not a value binding, so it qualifies; a user `function beforeAll`
/// or `const beforeAll = …` is in `locals` and is rejected, keeping ordinary
/// modules that shadow a hook name flagged.
fn is_test_runner_hook_call(
    call: &oxc_ast::ast::CallExpression,
    locals: &HashSet<String>,
) -> bool {
    let Some(root) = hook_callee_root(call) else {
        return false;
    };
    let is_hook = match &call.callee {
        Expression::StaticMemberExpression(_) => root == "expect",
        _ => TEST_RUNNER_HOOK_IDENTS.contains(&root),
    };
    is_hook && !locals.contains(root)
}

/// Names bound by top-level *value* declarations: `function`/`class`
/// declarations and `var`/`let`/`const` binding identifiers. Imports are
/// excluded — a hook imported from a runner package (`import { beforeAll } from
/// "vitest"`) is the runner's binding, not a user definition, so it must not
/// disqualify the setup-file shape.
fn module_top_level_value_bindings(program: &Program) -> HashSet<String> {
    let mut out = HashSet::new();
    for stmt in &program.body {
        match stmt {
            Statement::FunctionDeclaration(func) => {
                if let Some(id) = &func.id {
                    out.insert(id.name.to_string());
                }
            }
            Statement::ClassDeclaration(class) => {
                if let Some(id) = &class.id {
                    out.insert(id.name.to_string());
                }
            }
            Statement::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                        out.insert(id.name.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// True when the program is a test-runner setup file by content shape: it has at
/// least one top-level lifecycle-hook call statement and every top-level
/// call/`new` statement is such a hook (see `is_test_runner_hook_call`). Covers
/// both Jest setup files (hooks injected as globals, no import) and Vitest setup
/// files (hooks imported from `"vitest"`); a locally-defined hook name keeps the
/// module flagged. An empty program (no top-level call/`new` statements) returns
/// `false` — there is nothing to exempt.
fn shape_is_test_setup(program: &Program) -> bool {
    let locals = module_top_level_value_bindings(program);
    let mut seen_any = false;
    for stmt in &program.body {
        let Statement::ExpressionStatement(es) = stmt else { continue };
        match &es.expression {
            Expression::CallExpression(call) => {
                seen_any = true;
                if !is_test_runner_hook_call(call, &locals) {
                    return false;
                }
            }
            Expression::NewExpression(_) => return false,
            _ => {}
        }
    }
    seen_any
}

const VITEST_BENCH_REGISTRATION_IDENTS: &[&str] = &["describe", "bench"];

/// True when the program imports from `"vitest"` at the top level
/// (`import { bench, describe } from "vitest"`). Any import form from the
/// `vitest` package counts — the gate only needs to confirm the registration
/// identifiers come from the benchmark runner, not a local helper.
fn has_vitest_import(program: &Program) -> bool {
    program.body.iter().any(|stmt| {
        let Statement::ImportDeclaration(import) = stmt else { return false };
        import.source.value.as_str() == "vitest"
    })
}

/// True when the program is a Vitest benchmark file by content shape: it imports
/// from `"vitest"`, has at least one top-level `describe(...)`/`bench(...)`
/// registration call, and every top-level call/`new` statement is either such a
/// registration or a standard lifecycle hook (`beforeAll`/…). Vitest benchmark
/// files register their suites with top-level `describe`/`bench` calls exactly as
/// test files register with `it`/`beforeAll`; they are executed directly by
/// `vitest bench`, never imported, so the registrations are intentional, not a
/// tree-shaking hazard. The `vitest` import is required and a locally-bound
/// `describe`/`bench` name disqualifies the call, so an ordinary module that
/// merely calls a local `describe()` is still flagged.
fn is_vitest_bench_shape(program: &Program) -> bool {
    if !has_vitest_import(program) {
        return false;
    }
    let locals = module_top_level_value_bindings(program);
    let mut seen_registration = false;
    for stmt in &program.body {
        let Statement::ExpressionStatement(es) = stmt else { continue };
        match &es.expression {
            Expression::CallExpression(call) => {
                let is_registration = matches!(
                    &call.callee,
                    Expression::Identifier(id)
                        if VITEST_BENCH_REGISTRATION_IDENTS.contains(&id.name.as_str())
                            && !locals.contains(id.name.as_str())
                );
                if is_registration {
                    seen_registration = true;
                } else if !is_test_runner_hook_call(call, &locals) {
                    return false;
                }
            }
            Expression::NewExpression(_) => return false,
            _ => {}
        }
    }
    seen_registration
}

/// True when the program imports from a vanilla-extract package at the top level:
/// `@vanilla-extract/css` or any `@vanilla-extract/*` sub-package
/// (`@vanilla-extract/recipes`, `@vanilla-extract/dynamic`, …). Any import form
/// counts — the gate only needs to confirm the file uses the vanilla-extract API.
fn has_vanilla_extract_import(program: &Program) -> bool {
    program.body.iter().any(|stmt| {
        let Statement::ImportDeclaration(import) = stmt else { return false };
        let src = import.source.value.as_str();
        src == "@vanilla-extract" || src.starts_with("@vanilla-extract/")
    })
}

/// True when the program is a vanilla-extract style module: it imports from a
/// `@vanilla-extract/*` package. vanilla-extract is a zero-runtime CSS-in-TS
/// library — files using its API (`globalStyle(...)`, `style(...)`,
/// `keyframes(...)`, `recipe(...)`, …) are exclusively processed at compile time
/// by the vanilla-extract Vite/webpack plugin into static CSS and are never
/// bundled into runtime JavaScript. The API is designed around top-level calls,
/// so those calls are the file's whole purpose, not a tree-shaking hazard. The
/// `@vanilla-extract/*` import is the definitive signal so an arbitrary `.css.ts`
/// file that is not a vanilla-extract module is still flagged.
fn is_vanilla_extract_style_file(program: &Program) -> bool {
    has_vanilla_extract_import(program)
}

/// True when the call's callee is a server-startup entry call:
/// - `listen` (bare) or a `.listen` member access (`fastify.listen`,
///   `app.listen`, `server.listen`, …) — Fastify, Express, and Node's
///   `http.Server` all start the server with a `listen` call;
/// - `serve` (bare) — the canonical Deno std-library Edge Function entry
///   (`serve(handler)` from `deno.land/std/http/server.ts`), the runtime
///   equivalent of `listen`;
/// - `Deno.serve` or `Bun.serve` — the modern Deno/Bun runtime server entry
///   points. The receiver root is matched precisely (`Deno`/`Bun`) so an
///   unrelated `obj.serve(...)` does not gain the exemption.
fn is_listen_call(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => id.name == "listen" || id.name == "serve",
        Expression::StaticMemberExpression(m) => {
            m.property.name == "listen"
                || (m.property.name == "serve"
                    && matches!(
                        &m.object,
                        Expression::Identifier(obj) if obj.name == "Deno" || obj.name == "Bun"
                    ))
        }
        _ => false,
    }
}

/// True when `call` is a `listen(...)` call, or a `.catch(...)`/`.then(...)`
/// continuation chained onto one. The idiomatic Node.js server startup pattern
/// attaches error handling to the listen promise — `app.listen({port}).catch(…)`
/// or `app.listen({port}).then(…)` — so the top-level expression is a `.catch`
/// member call whose object is the `listen` call. Unwrapping the trailing
/// continuation recovers the underlying `listen`, matching both the bare call
/// and the chained forms.
fn call_chain_starts_with_listen(call: &oxc_ast::ast::CallExpression) -> bool {
    if is_listen_call(call) {
        return true;
    }
    let Expression::StaticMemberExpression(m) = &call.callee else {
        return false;
    };
    if !matches!(m.property.name.as_str(), "catch" | "then") {
        return false;
    }
    let Expression::CallExpression(inner) = &m.object else {
        return false;
    };
    call_chain_starts_with_listen(inner)
}

/// True when `stmt` is an expression statement whose expression is a
/// `listen(...)` call (or a `.catch(...)`/`.then(...)` continuation chained onto
/// one — see `call_chain_starts_with_listen`).
fn is_listen_call_statement(stmt: &Statement) -> bool {
    let Statement::ExpressionStatement(es) = stmt else { return false };
    let Expression::CallExpression(call) = &es.expression else { return false };
    call_chain_starts_with_listen(call)
}

/// True when an `if` branch (consequent or alternate) contains a `listen(...)`
/// call statement (see `is_listen_call_statement`): either the braced form
/// (`{ server.listen({ fd }) }`) or the braceless single-statement form
/// (`if (x) server.listen(fd)`). Used to look one level into a top-level `if` for
/// the socket-activation pattern.
fn branch_contains_listen_call(branch: &Statement) -> bool {
    match branch {
        Statement::BlockStatement(block) => block.body.iter().any(is_listen_call_statement),
        other => is_listen_call_statement(other),
    }
}

/// True when the program is a server application entry point by content shape: it
/// has a server-startup call statement (`listen`, bare `serve`, `Deno.serve`,
/// `Bun.serve` — see `is_listen_call`) either at the top level
/// (`fastify.listen({ port })`, `Deno.serve(handler)`) or inside the
/// consequent/alternate of a top-level `if` statement (the socket-activation
/// pattern: `if (socketActivation) { server.listen({ fd }) } else {
/// server.listen({ port }) }`). Starting an HTTP server this way makes the
/// surrounding route/hook/middleware/signal-handler registrations mandatory,
/// intentional side effects, not tree-shakeable library code. A startup call
/// chained with a `.catch(...)`/`.then(...)` continuation counts the same. Only
/// the top level and the first level of `if` branches are inspected, so a startup
/// call buried inside an unrelated function body does not grant the exemption.
fn is_server_entry_shape(program: &Program) -> bool {
    program.body.iter().any(|stmt| {
        if is_listen_call_statement(stmt) {
            return true;
        }
        let Statement::IfStatement(if_stmt) = stmt else { return false };
        branch_contains_listen_call(&if_stmt.consequent)
            || if_stmt.alternate.as_ref().is_some_and(branch_contains_listen_call)
    })
}

/// True when `call` is a bare `main()` call, or a `.catch(...)`/`.then(...)`
/// continuation chained onto one. CLI entry points commonly invoke the program
/// through `main().catch(err => { … })` to surface async failures, so the
/// top-level expression is a `.catch` member call whose object is the `main`
/// call. Unwrapping the trailing continuation recovers the underlying `main`,
/// matching both the bare call and the chained forms.
fn call_chain_starts_with_main(call: &oxc_ast::ast::CallExpression) -> bool {
    if matches!(&call.callee, Expression::Identifier(id) if id.name == "main") {
        return true;
    }
    let Expression::StaticMemberExpression(m) = &call.callee else {
        return false;
    };
    if !matches!(m.property.name.as_str(), "catch" | "then") {
        return false;
    }
    let Expression::CallExpression(inner) = &m.object else {
        return false;
    };
    call_chain_starts_with_main(inner)
}

/// True when the program is a Node.js CLI script entry point that runs itself by
/// invoking a locally-defined `main` function at the top level — the
/// conventional CLI shape `async function main() { … }` followed by `main()` (or
/// the `main().catch(err => { … })` form that surfaces async failures). Such a
/// file is executed directly (`tsx ./index.mts`, `node ./cli.js`), never imported
/// as a library module, so its top-level entry invocation is the program's
/// purpose, not a tree-shakeable side effect.
///
/// Both signals are required: a top-level `main(...)` call statement AND a
/// top-level value declaration binding `main` (a `function main`/`const main =
/// …` in this module). The local-definition requirement keeps the exemption
/// precise — a bare `register()` or a `main()` call whose `main` is imported from
/// another module is not this self-invocation pattern and is still flagged.
fn is_cli_main_entry_shape(program: &Program) -> bool {
    if !module_top_level_value_bindings(program).contains("main") {
        return false;
    }
    program.body.iter().any(|stmt| {
        let Statement::ExpressionStatement(es) = stmt else { return false };
        let Expression::CallExpression(call) = &es.expression else { return false };
        call_chain_starts_with_main(call)
    })
}

/// True when `call`'s callee is `name` (bare identifier) or a `.name` member
/// access (`ReactDOM.createRoot`, `client.hydrateRoot`, …).
fn callee_is_ident_or_member(
    call: &oxc_ast::ast::CallExpression,
    name: &str,
) -> bool {
    match &call.callee {
        Expression::Identifier(id) => id.name == name,
        Expression::StaticMemberExpression(m) => m.property.name == name,
        _ => false,
    }
}

/// True when `call` is a React DOM bootstrap call statement:
/// - `createRoot(...).render(...)` / `ReactDOM.createRoot(...).render(...)`;
/// - `hydrateRoot(...)` / `ReactDOM.hydrateRoot(...)`;
/// - legacy `ReactDOM.render(...)` (member call on a `ReactDOM` object).
///
/// A bare `something.render(...)` is intentionally NOT matched — only the
/// chained `createRoot().render()` form and the `ReactDOM.render()` form count,
/// so unrelated `.render()` methods don't slip through.
fn is_react_bootstrap_call(call: &oxc_ast::ast::CallExpression) -> bool {
    if callee_is_ident_or_member(call, "hydrateRoot") {
        return true;
    }
    // `createRoot(...).render(...)`: outer callee is a `.render` member whose
    // object is itself a `createRoot(...)` call.
    if let Expression::StaticMemberExpression(m) = &call.callee {
        if m.property.name == "render"
            && let Expression::CallExpression(inner) = &m.object
            && callee_is_ident_or_member(inner, "createRoot")
        {
            return true;
        }
        // Legacy `ReactDOM.render(...)`: `.render` on a `ReactDOM`/`ReactDom`
        // identifier object.
        if m.property.name == "render"
            && let Expression::Identifier(obj) = &m.object
            && matches!(obj.name.as_str(), "ReactDOM" | "ReactDom")
        {
            return true;
        }
    }
    false
}

/// True when the program has a top-level React DOM bootstrap call statement.
/// Such a module is a React application entry point (`main.tsx`, `index.tsx`,
/// `entry.client.tsx`, …): mounting the app at module level is the file's whole
/// purpose, and entry points are never imported by other modules, so the
/// surrounding top-level setup is an intentional side effect, not tree-shakeable
/// library code.
fn is_react_entry_shape(program: &Program) -> bool {
    program.body.iter().any(|stmt| {
        let Statement::ExpressionStatement(es) = stmt else { return false };
        let Expression::CallExpression(call) = &es.expression else { return false };
        is_react_bootstrap_call(call)
    })
}

/// True when the program imports the `render` binding from `"solid-js/web"` at
/// the top level (`import { render } from "solid-js/web"`). The imported symbol
/// name must be `render`; an alias (`import { render as r }`) still counts.
fn has_solid_render_import(program: &Program) -> bool {
    program.body.iter().any(|stmt| {
        let Statement::ImportDeclaration(import) = stmt else { return false };
        if import.source.value.as_str() != "solid-js/web" {
            return false;
        }
        let Some(specifiers) = &import.specifiers else { return false };
        specifiers.iter().any(|spec| {
            matches!(
                spec,
                ImportDeclarationSpecifier::ImportSpecifier(named)
                    if named.imported.name() == "render"
            )
        })
    })
}

/// True when the program is a Solid.js application entry point: it imports the
/// `render` binding from `"solid-js/web"` and calls it at the top level
/// (`render(() => <App />, root)`). `render` is Solid.js's app bootstrap
/// (analogous to React's `createRoot().render()` / Vue's `createApp().mount()`):
/// mounting the app at module level is the entry file's whole purpose, and entry
/// points are never imported by other modules, so the top-level call is an
/// intentional side effect, not tree-shakeable library code. The `solid-js/web`
/// import is required so an ordinary module that happens to call a local
/// `render()` is still flagged.
fn is_solid_entry_shape(program: &Program) -> bool {
    if !has_solid_render_import(program) {
        return false;
    }
    program.body.iter().any(|stmt| {
        let Statement::ExpressionStatement(es) = stmt else { return false };
        let Expression::CallExpression(call) = &es.expression else { return false };
        matches!(&call.callee, Expression::Identifier(id) if id.name == "render")
    })
}

/// True when any expression in `expr`'s member/call chain is a `createApp(...)`
/// call (a `createApp(...)` whose callee is the bare identifier `createApp`).
///
/// Walks the receiver chain so both the chained form
/// (`createApp(App).use(router).mount('#app')`) and a bare `createApp(App)`
/// declaration init are recognized.
fn chain_contains_create_app_call(expr: &Expression) -> bool {
    let mut current = expr;
    loop {
        match current {
            Expression::CallExpression(call) => {
                if matches!(&call.callee, Expression::Identifier(id) if id.name == "createApp") {
                    return true;
                }
                current = &call.callee;
            }
            Expression::StaticMemberExpression(m) => current = &m.object,
            Expression::ComputedMemberExpression(m) => current = &m.object,
            _ => return false,
        }
    }
}

/// True when any call in `expr`'s member/call chain is a `.mount(...)` member
/// call (`app.mount('#app')` or the chained `createApp(App).mount('#app')`).
fn chain_contains_mount_call(expr: &Expression) -> bool {
    let mut current = expr;
    loop {
        match current {
            Expression::CallExpression(call) => {
                if matches!(&call.callee, Expression::StaticMemberExpression(m) if m.property.name == "mount")
                {
                    return true;
                }
                current = &call.callee;
            }
            Expression::StaticMemberExpression(m) => current = &m.object,
            Expression::ComputedMemberExpression(m) => current = &m.object,
            _ => return false,
        }
    }
}

/// True when the program is a Vue 3 application entry point (`main.ts`,
/// `main.tsx`, …): it both creates the app at module scope with a top-level
/// `createApp(...)` call and mounts it with a top-level `.mount(...)` call,
/// covering the split form (`const app = createApp(App); app.use(router);
/// app.mount('#app')`) and the chained form
/// (`createApp(App).use(router).mount('#app')`). Mounting the app at module
/// level is the entry file's whole purpose, and entry points are never imported
/// by other modules, so the surrounding `app.use` / `app.component` /
/// `app.provide` registrations are intentional side effects, not tree-shakeable
/// library code. Requiring both `createApp` and `.mount` keeps an ordinary
/// module that merely calls a local `createApp()` helper, or some unrelated
/// `.mount()`, still flagged.
fn is_vue_entry_shape(program: &Program) -> bool {
    let has_create_app = program.body.iter().any(|stmt| match stmt {
        Statement::ExpressionStatement(es) => chain_contains_create_app_call(&es.expression),
        Statement::VariableDeclaration(decl) => decl
            .declarations
            .iter()
            .any(|d| d.init.as_ref().is_some_and(chain_contains_create_app_call)),
        _ => false,
    });
    if !has_create_app {
        return false;
    }
    program.body.iter().any(|stmt| {
        let Statement::ExpressionStatement(es) = stmt else { return false };
        chain_contains_mount_call(&es.expression)
    })
}

/// True when the program imports the `render` binding from `"preact"` at the top
/// level (`import { render } from "preact"`). The imported symbol name must be
/// `render`; an alias (`import { render as r }`) still counts.
fn has_preact_render_import(program: &Program) -> bool {
    program.body.iter().any(|stmt| {
        let Statement::ImportDeclaration(import) = stmt else { return false };
        if import.source.value.as_str() != "preact" {
            return false;
        }
        let Some(specifiers) = &import.specifiers else { return false };
        specifiers.iter().any(|spec| {
            matches!(
                spec,
                ImportDeclarationSpecifier::ImportSpecifier(named)
                    if named.imported.name() == "render"
            )
        })
    })
}

/// True when the program is a Preact application entry point: it imports the
/// `render` binding from `"preact"` and calls it at the top level
/// (`render(<App />, document.getElementById('app')!)`). `render` is Preact's app
/// bootstrap (analogous to React's `createRoot().render()` / Vue's
/// `createApp().mount()`): mounting the app at module level is the entry file's
/// whole purpose, and entry points are never imported by other modules, so the
/// top-level call is an intentional side effect, not tree-shakeable library code.
/// The `preact` import is required so an ordinary module that happens to call a
/// local `render()` is still flagged.
fn is_preact_entry_shape(program: &Program) -> bool {
    if !has_preact_render_import(program) {
        return false;
    }
    program.body.iter().any(|stmt| {
        let Statement::ExpressionStatement(es) = stmt else { return false };
        let Expression::CallExpression(call) = &es.expression else { return false };
        matches!(&call.callee, Expression::Identifier(id) if id.name == "render")
    })
}

/// True when the program imports the `bootstrapApplication` binding from
/// `"@angular/platform-browser"` at the top level
/// (`import { bootstrapApplication } from "@angular/platform-browser"`). The
/// imported symbol name must be `bootstrapApplication`; an alias still counts.
fn has_angular_bootstrap_import(program: &Program) -> bool {
    program.body.iter().any(|stmt| {
        let Statement::ImportDeclaration(import) = stmt else { return false };
        if import.source.value.as_str() != "@angular/platform-browser" {
            return false;
        }
        let Some(specifiers) = &import.specifiers else { return false };
        specifiers.iter().any(|spec| {
            matches!(
                spec,
                ImportDeclarationSpecifier::ImportSpecifier(named)
                    if named.imported.name() == "bootstrapApplication"
            )
        })
    })
}

/// True when `call` is a bare `bootstrapApplication(...)` call, or a
/// `.catch(...)`/`.then(...)` continuation chained onto one. The canonical
/// Angular standalone entry attaches error handling to the bootstrap promise —
/// `bootstrapApplication(App, appConfig).catch(err => console.error(err))` — so
/// the top-level expression is a `.catch` member call whose object is the
/// `bootstrapApplication` call. Unwrapping the trailing continuation recovers the
/// underlying call, matching both the bare and chained forms.
fn call_chain_starts_with_bootstrap_application(
    call: &oxc_ast::ast::CallExpression,
) -> bool {
    if matches!(&call.callee, Expression::Identifier(id) if id.name == "bootstrapApplication") {
        return true;
    }
    let Expression::StaticMemberExpression(m) = &call.callee else {
        return false;
    };
    if !matches!(m.property.name.as_str(), "catch" | "then") {
        return false;
    }
    let Expression::CallExpression(inner) = &m.object else {
        return false;
    };
    call_chain_starts_with_bootstrap_application(inner)
}

/// True when the program is an Angular standalone application entry point
/// (`main.ts`): it imports `bootstrapApplication` from
/// `"@angular/platform-browser"` and calls it at the top level
/// (`bootstrapApplication(AppComponent, appConfig)`, optionally chained with
/// `.catch(...)`/`.then(...)` to surface bootstrap failures).
/// `bootstrapApplication` is Angular's standalone app bootstrap (analogous to
/// React's `createRoot().render()` / Vue's `createApp().mount()`): bootstrapping
/// the app at module level is the entry file's whole purpose, and entry points
/// are never imported by other modules, so the top-level call is an intentional
/// side effect, not tree-shakeable library code. The `@angular/platform-browser`
/// import is required so an ordinary module that happens to call a local
/// `bootstrapApplication()` is still flagged.
fn is_angular_entry_shape(program: &Program) -> bool {
    if !has_angular_bootstrap_import(program) {
        return false;
    }
    program.body.iter().any(|stmt| {
        let Statement::ExpressionStatement(es) = stmt else { return false };
        let Expression::CallExpression(call) = &es.expression else { return false };
        call_chain_starts_with_bootstrap_application(call)
    })
}

const GULP_REGISTRATION_IDENTS: &[&str] = &["task", "series", "parallel", "watch"];

/// True when the program imports the `gulp` module at the top level, in any
/// form: an ESM `import` from `"gulp"` (or a `"gulp/"` sub-path), or a
/// `require("gulp")` / `require("gulp/...")` call in a top-level declaration.
fn has_gulp_import(program: &Program) -> bool {
    fn is_gulp_specifier(src: &str) -> bool {
        src == "gulp" || src.starts_with("gulp/")
    }
    program.body.iter().any(|stmt| match stmt {
        Statement::ImportDeclaration(import) => is_gulp_specifier(import.source.value.as_str()),
        Statement::VariableDeclaration(decl) => decl.declarations.iter().any(|d| {
            let Some(Expression::CallExpression(call)) = &d.init else { return false };
            let Expression::Identifier(id) = &call.callee else { return false };
            if id.name != "require" {
                return false;
            }
            matches!(call.arguments.first(), Some(Argument::StringLiteral(s)) if is_gulp_specifier(s.value.as_str()))
        }),
        _ => false,
    })
}

/// True when `call`'s callee is a Gulp task-registration function — either a
/// bare identifier (`task(...)`, `series(...)`, `parallel(...)`, `watch(...)`)
/// or the same property on a `gulp` object (`gulp.task(...)`, …).
fn is_gulp_registration_call(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => GULP_REGISTRATION_IDENTS.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(m) => {
            let Expression::Identifier(obj) = &m.object else { return false };
            obj.name == "gulp" && GULP_REGISTRATION_IDENTS.contains(&m.property.name.as_str())
        }
        _ => false,
    }
}

/// True when the program is a Gulp task-registration file: it imports `gulp`
/// and registers at least one task at the top level (`task(...)`,
/// `gulp.task(...)`, `series(...)`, `parallel(...)`, …). Such a file is a
/// build-script entry point consumed by the Gulp task runner — importing it to
/// run the registrations is its sole purpose, so the top-level calls are
/// intentional side effects, not tree-shakeable library code. The `gulp` import
/// is required so an ordinary module that happens to call a local `task()` is
/// still flagged.
fn is_gulp_task_file(program: &Program) -> bool {
    if !has_gulp_import(program) {
        return false;
    }
    program.body.iter().any(|stmt| {
        let Statement::ExpressionStatement(es) = stmt else { return false };
        let Expression::CallExpression(call) = &es.expression else { return false };
        is_gulp_registration_call(call)
    })
}

const STORYBOOK_MANAGER_SPECIFIERS: &[&str] = &[
    "@storybook/manager-api",
    "@storybook/addons",
    "@storybook/manager",
    "storybook/manager-api",
];

const STORYBOOK_REGISTRATION_IDENTS: &[&str] = &["register", "add", "setConfig"];

/// Collect local identifier names bound to the default/namespace/`addons`
/// import from a Storybook manager package. Handles
/// `import { addons } from "storybook/manager-api"`,
/// `import { addons as a } from "@storybook/manager-api"`,
/// `import addons from "@storybook/addons"`, and
/// `import * as addons from "@storybook/manager-api"`.
fn storybook_addons_bindings(program: &Program) -> HashSet<String> {
    let mut out = HashSet::new();
    for stmt in &program.body {
        let Statement::ImportDeclaration(import) = stmt else { continue };
        if !STORYBOOK_MANAGER_SPECIFIERS.contains(&import.source.value.as_str()) {
            continue;
        }
        let Some(specifiers) = &import.specifiers else { continue };
        for spec in specifiers {
            match spec {
                ImportDeclarationSpecifier::ImportSpecifier(named)
                    if named.imported.name() == "addons" =>
                {
                    out.insert(named.local.name.to_string());
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => {
                    out.insert(def.local.name.to_string());
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => {
                    out.insert(ns.local.name.to_string());
                }
                _ => {}
            }
        }
    }
    out
}

/// True when `call` registers a Storybook addon at the top level —
/// `addons.register(...)`, `addons.add(...)`, or `addons.setConfig(...)` where
/// `addons` is the binding imported from a Storybook manager package.
fn is_storybook_registration_call(
    call: &oxc_ast::ast::CallExpression,
    bindings: &HashSet<String>,
) -> bool {
    let Expression::StaticMemberExpression(m) = &call.callee else {
        return false;
    };
    if !STORYBOOK_REGISTRATION_IDENTS.contains(&m.property.name.as_str()) {
        return false;
    }
    let Expression::Identifier(obj) = &m.object else {
        return false;
    };
    bindings.contains(obj.name.as_str())
}

/// True when the program is a Storybook addon manager entry file: it imports
/// the `addons` API from a Storybook manager package and registers the addon at
/// the top level (`addons.register(...)`, `addons.add(...)`,
/// `addons.setConfig(...)`). Such a file is an extension-point entry — the
/// Storybook manager bundle loads it by glob/import specifically to run those
/// registrations — so the top-level calls are intentional side effects, not
/// tree-shakeable library code. The Storybook import is required so an ordinary
/// module that happens to call a local `addons.register()` is still flagged.
fn is_storybook_addon_file(program: &Program) -> bool {
    let bindings = storybook_addons_bindings(program);
    if bindings.is_empty() {
        return false;
    }
    program.body.iter().any(|stmt| {
        let Statement::ExpressionStatement(es) = stmt else { return false };
        let Expression::CallExpression(call) = &es.expression else { return false };
        is_storybook_registration_call(call, &bindings)
    })
}

/// True when the program imports from the MCP SDK package
/// (`@modelcontextprotocol/sdk` or any `@modelcontextprotocol/sdk/` sub-path
/// such as `@modelcontextprotocol/sdk/server/index.js`).
fn has_mcp_sdk_import(program: &Program) -> bool {
    fn is_mcp_specifier(src: &str) -> bool {
        src == "@modelcontextprotocol/sdk" || src.starts_with("@modelcontextprotocol/sdk/")
    }
    program.body.iter().any(|stmt| {
        let Statement::ImportDeclaration(import) = stmt else { return false };
        is_mcp_specifier(import.source.value.as_str())
    })
}

/// True when `call` registers an MCP request handler:
/// `<receiver>.setRequestHandler(...)` (a `.setRequestHandler` member call).
fn is_set_request_handler_call(call: &oxc_ast::ast::CallExpression) -> bool {
    matches!(
        &call.callee,
        Expression::StaticMemberExpression(m) if m.property.name == "setRequestHandler"
    )
}

/// True when the program is an MCP (Model Context Protocol) server module: it
/// imports from `@modelcontextprotocol/sdk` and registers a request handler at
/// the top level (`server.setRequestHandler(Schema, handler)`). The MCP SDK
/// requires handlers to be registered at module scope on the server instance,
/// which is exported and consumed by the transport layer — like the `listen()`
/// server-entry shape, the registrations are intentional initialization of the
/// exported server's API, not tree-shakeable library code. The MCP SDK import is
/// required so a stray `.setRequestHandler` on an unrelated object in a non-MCP
/// module is still flagged.
fn is_mcp_server_file(program: &Program) -> bool {
    if !has_mcp_sdk_import(program) {
        return false;
    }
    program.body.iter().any(|stmt| {
        let Statement::ExpressionStatement(es) = stmt else { return false };
        let Expression::CallExpression(call) = &es.expression else { return false };
        is_set_request_handler_call(call)
    })
}

/// True when the program imports a Node filesystem module at the top level, in
/// any form: an ESM `import` from `fs` / `node:fs` / `fs/promises` /
/// `node:fs/promises`, or a `require("fs")` / `require("node:fs")` /
/// `require("fs/promises")` call in a top-level declaration. Filesystem writes
/// are the defining capability of a code-generation script.
fn has_fs_import(program: &Program) -> bool {
    fn is_fs_specifier(src: &str) -> bool {
        matches!(src, "fs" | "node:fs" | "fs/promises" | "node:fs/promises")
    }
    program.body.iter().any(|stmt| match stmt {
        Statement::ImportDeclaration(import) => is_fs_specifier(import.source.value.as_str()),
        Statement::VariableDeclaration(decl) => decl.declarations.iter().any(|d| {
            let Some(Expression::CallExpression(call)) = &d.init else { return false };
            let Expression::Identifier(id) = &call.callee else { return false };
            if id.name != "require" {
                return false;
            }
            matches!(call.arguments.first(), Some(Argument::StringLiteral(s)) if is_fs_specifier(s.value.as_str()))
        }),
        _ => false,
    })
}

/// True when the program is a code-generation utility script: it imports a Node
/// filesystem module and every one of its top-level expression statements is a
/// bare-identifier call to the *same* function, invoked at least twice (e.g.
/// `run(arSA, 'ar-SA'); run(beBY, 'be-BY'); …`). Such a file iterates one
/// processing function over a dataset to write output files — its whole purpose
/// is the top-level work. It is executed directly (`tsx generate.ts`), never
/// imported as a library module, so the repeated top-level calls are intentional,
/// not a tree-shaking hazard.
///
/// The uniform same-callee shape plus the filesystem import keep the exemption
/// narrow: a module with a single top-level call, or with heterogeneous
/// top-level calls, or one that never touches the filesystem is still flagged.
fn is_data_generation_script(program: &Program) -> bool {
    if !has_fs_import(program) {
        return false;
    }
    let mut callee: Option<&str> = None;
    let mut count = 0usize;
    for stmt in &program.body {
        let Statement::ExpressionStatement(es) = stmt else { continue };
        let Expression::CallExpression(call) = &es.expression else {
            return false;
        };
        let Expression::Identifier(id) = &call.callee else {
            return false;
        };
        match callee {
            None => callee = Some(id.name.as_str()),
            Some(name) if name == id.name.as_str() => {}
            Some(_) => return false,
        }
        count += 1;
    }
    count >= 2
}

/// Collect local identifier names that are bound to `startTransition`
/// imported from `"react"`. Handles `import { startTransition } from "react"`
/// and `import { startTransition as ST } from "react"`.
fn react_start_transition_bindings(program: &Program) -> HashSet<String> {
    let mut out = HashSet::new();
    for stmt in &program.body {
        let Statement::ImportDeclaration(import) = stmt else { continue };
        if import.source.value.as_str() != "react" {
            continue;
        }
        let Some(specifiers) = &import.specifiers else { continue };
        for spec in specifiers {
            let ImportDeclarationSpecifier::ImportSpecifier(named) = spec else {
                continue;
            };
            if named.imported.name() == "startTransition" {
                out.insert(named.local.name.to_string());
            }
        }
    }
    out
}

fn is_start_transition_call(
    call: &oxc_ast::ast::CallExpression,
    bindings: &HashSet<String>,
) -> bool {
    let Expression::Identifier(id) = &call.callee else { return false };
    bindings.contains(id.name.as_str())
}

/// Vue Composition API / VueUse reactivity-setup primitives. A top-level call to
/// one of these wires reactive state together when the module is first imported —
/// a declarative reactive registration, not a tree-shakeable side effect.
const VUE_REACTIVITY_IDENTS: &[&str] = &[
    "watch",
    "watchEffect",
    "watchPostEffect",
    "watchSyncEffect",
    "debouncedWatch",
    "watchDebounced",
    "watchThrottled",
    "watchOnce",
];

/// True when `src` is a Vue ecosystem package — `vue`, `@vue/*` (e.g.
/// `@vue/reactivity`, `@vue/runtime-core`), or `@vueuse/core`. The reactivity
/// primitives that drive the [`VUE_REACTIVITY_IDENTS`] exemption ship from these
/// packages.
fn is_vue_ecosystem_specifier(src: &str) -> bool {
    src == "vue" || src == "@vueuse/core" || src.starts_with("@vue/")
}

/// Collect local identifier names bound to a Vue reactivity primitive imported
/// from a Vue ecosystem package. Handles `import { watch } from "vue"` and
/// `import { watchEffect as track } from "@vue/runtime-core"`. The import source
/// gate keeps a same-named local `watch()` (or one imported from `node:fs`) out
/// of the set, so only the genuine Vue idiom is exempted.
fn vue_reactivity_bindings(program: &Program) -> HashSet<String> {
    let mut out = HashSet::new();
    for stmt in &program.body {
        let Statement::ImportDeclaration(import) = stmt else { continue };
        if !is_vue_ecosystem_specifier(import.source.value.as_str()) {
            continue;
        }
        let Some(specifiers) = &import.specifiers else { continue };
        for spec in specifiers {
            let ImportDeclarationSpecifier::ImportSpecifier(named) = spec else {
                continue;
            };
            if VUE_REACTIVITY_IDENTS.contains(&named.imported.name().as_str()) {
                out.insert(named.local.name.to_string());
            }
        }
    }
    out
}

/// True when `call` is a top-level Vue reactivity-setup call — a bare-identifier
/// call (`watch(...)`, `debouncedWatch(...)`) whose callee is bound to a Vue
/// reactivity primitive imported from a Vue ecosystem package (see
/// [`vue_reactivity_bindings`]).
fn is_vue_reactivity_setup_call(
    call: &oxc_ast::ast::CallExpression,
    bindings: &HashSet<String>,
) -> bool {
    let Expression::Identifier(id) = &call.callee else { return false };
    bindings.contains(id.name.as_str())
}

/// True when `call` registers a Node.js process signal handler at the top level:
/// `process.on("SIG...", handler)` (`process.on("SIGINT", …)`,
/// `process.on("SIGTERM", …)`, …). CLI entry points register these so the
/// program can shut down gracefully on interruption — the registration is the
/// program's intended setup, not a tree-shakeable side effect. The receiver must
/// be the bare `process` global, the method must be `on`, and the first argument
/// must be a string literal whose name starts with `SIG`, so a `.on(...)`
/// subscription on any other emitter, or a non-signal event, is still flagged.
fn is_process_signal_handler_call(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(m) = &call.callee else {
        return false;
    };
    if m.property.name != "on" {
        return false;
    }
    let Expression::Identifier(obj) = &m.object else {
        return false;
    };
    if obj.name != "process" {
        return false;
    }
    matches!(
        call.arguments.first().and_then(Argument::as_expression),
        Some(Expression::StringLiteral(s)) if s.value.as_str().starts_with("SIG")
    )
}

/// True when `call` is a library plugin-registration call: a member call
/// `recv.<name>(...)` whose method is one of a small curated set of unambiguous
/// plugin-registration idioms — `extend` (dayjs's `dayjs.extend(plugin)`) or
/// `registerPlugin` (gsap/FilePond's `gsap.registerPlugin(x)`). These are the
/// documented, required one-time configuration to enable a library feature: the
/// call is idempotent, performs no external I/O, and merely mutates the library's
/// own singleton, so it is intentional module configuration, not a tree-shaking
/// hazard. The receiver is not pinned to a specific library name, but the method
/// allowlist is kept deliberately narrow: common method names like `use`,
/// `register`, and `add` are excluded because they overwhelmingly denote genuine
/// side effects (`app.use(mw)`, arbitrary `x.register(...)`) and would
/// over-exempt. So a non-registration member call (`analytics.track(...)`,
/// `db.connect()`, `app.use(mw)`) is still flagged.
fn is_library_plugin_registration_call(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(m) = &call.callee else {
        return false;
    };
    matches!(m.property.name.as_str(), "extend" | "registerPlugin")
}

/// True when `call` is a commander.js subcommand-registration builder chain:
/// a fluent method chain that both registers a subcommand with `.command(...)`
/// and attaches its handler with `.action(...)` (e.g.
/// `mcp.command("init").description(...).option(...).action(handler)`).
/// commander requires subcommands to be assembled at module scope on a
/// `Command` instance, so the chain is the program's intended setup, observed
/// only through the configured command object, not a tree-shakeable side effect.
/// Requiring both `.command` and `.action` keeps the shape precise: an unrelated
/// fluent chain that merely has a `.command(...)` or `.action(...)` method (but
/// not both) is still flagged.
fn is_commander_subcommand_chain(call: &oxc_ast::ast::CallExpression) -> bool {
    let mut saw_command = false;
    let mut saw_action = false;
    let mut current: &Expression = &call.callee;
    // Record the method of the outermost call, then walk the receiver chain
    // recording each subsequent `.method(...)` call.
    if let Expression::StaticMemberExpression(m) = &call.callee {
        match m.property.name.as_str() {
            "command" => saw_command = true,
            "action" => saw_action = true,
            _ => {}
        }
    }
    loop {
        match current {
            Expression::CallExpression(inner) => {
                if let Expression::StaticMemberExpression(m) = &inner.callee {
                    match m.property.name.as_str() {
                        "command" => saw_command = true,
                        "action" => saw_action = true,
                        _ => {}
                    }
                }
                current = &inner.callee;
            }
            Expression::StaticMemberExpression(m) => current = &m.object,
            Expression::ComputedMemberExpression(m) => current = &m.object,
            _ => break,
        }
    }
    saw_command && saw_action
}

/// True when `path` is a TanStack Start entry file: `app/client.{ts,tsx}`,
/// `app/router.{ts,tsx}`, or `app/server.{ts,tsx}` (also under `src/app/`).
/// Requires the project to have the `tanstack-router` framework detected.
fn is_tanstack_start_entry(path: &std::path::Path, project: &crate::project::ProjectCtx) -> bool {
    if !project.has_framework("tanstack-router") {
        return false;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem = if let Some(s) = name.strip_suffix(".tsx") {
        s
    } else if let Some(s) = name.strip_suffix(".ts") {
        s
    } else {
        return false;
    };
    if !matches!(stem, "client" | "router" | "server") {
        return false;
    }
    let s = path.to_string_lossy().replace('\\', "/");
    s.contains("/app/client.") || s.contains("/app/router.") || s.contains("/app/server.")
        || s == "app/client.ts" || s == "app/client.tsx"
        || s == "app/router.ts" || s == "app/router.tsx"
        || s == "app/server.ts" || s == "app/server.tsx"
}

/// Collect the names of identifiers bound by top-level `const` declarations
/// with a simple binding (`const lookup = …`). Destructuring patterns are
/// skipped — the data-init exemption only reasons about plain named bindings.
fn module_const_bindings(program: &Program) -> HashSet<String> {
    let mut out = HashSet::new();
    for stmt in &program.body {
        let decl = match stmt {
            Statement::VariableDeclaration(decl) => decl,
            _ => continue,
        };
        if decl.kind != oxc_ast::ast::VariableDeclarationKind::Const {
            continue;
        }
        for declarator in &decl.declarations {
            if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                out.insert(id.name.to_string());
            }
        }
    }
    out
}

/// Collect the names of identifiers bound by top-level
/// `const <name> = new ...()` declarations. These are freshly-constructed
/// objects local to the module; configuring them in place is the builder/fluent
/// pattern, not an external side effect.
fn module_const_new_bindings(program: &Program) -> HashSet<String> {
    let mut out = HashSet::new();
    for stmt in &program.body {
        let Statement::VariableDeclaration(decl) = stmt else { continue };
        if decl.kind != oxc_ast::ast::VariableDeclarationKind::Const {
            continue;
        }
        for declarator in &decl.declarations {
            let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                continue;
            };
            if matches!(declarator.init, Some(Expression::NewExpression(_))) {
                out.insert(id.name.to_string());
            }
        }
    }
    out
}

/// True when `call` configures a freshly-constructed module-local object:
/// `obj.method(...)` where `obj` is a bare identifier bound by a top-level
/// `const obj = new ...()`. This is the builder/fluent configuration pattern —
/// all mutation targets an object created in the same file that has not escaped,
/// so there is no external side effect and no tree-shaking hazard. The receiver
/// must be a bare identifier (not a longer chain or a call result), so an
/// imported singleton (`registry.register(...)`) or a `const x = getThing()`
/// whose init is a plain call is still flagged.
fn is_local_const_config_call(
    call: &oxc_ast::ast::CallExpression,
    new_locals: &HashSet<String>,
) -> bool {
    let Expression::StaticMemberExpression(m) = &call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &m.object else {
        return false;
    };
    new_locals.contains(obj.name.as_str())
}

/// Collect the local binding names this module re-exports: the identifier of a
/// `export default <id>` and the *local* name of every named export specifier
/// (`export { styled }` → `styled`, `export { foo as bar }` → `foo`). These are
/// the bindings whose object is the module's own public contract, so augmenting
/// one before consumers observe it is module initialization, not external state.
fn module_exported_local_bindings(program: &Program) -> HashSet<String> {
    let mut out = HashSet::new();
    for stmt in &program.body {
        match stmt {
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::Identifier(id) = &export.declaration {
                    out.insert(id.name.to_string());
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                for spec in &export.specifiers {
                    if let Some(local) = spec.local.identifier_name() {
                        out.insert(local.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// True when `call` augments the module's own export object:
/// `Object.assign(target, ...)` where `target` is either a bare identifier this
/// module re-exports (`exported`) or the CommonJS `exports` / `module.exports`
/// object. Such a call initializes the module's public contract before any
/// consumer observes it, so it is no external side effect. `Object.assign` onto
/// an imported singleton, a global (`window`/`globalThis`), or any non-exported
/// binding is not matched and stays flagged.
fn is_export_object_assign(
    call: &oxc_ast::ast::CallExpression,
    exported: &HashSet<String>,
) -> bool {
    let Expression::StaticMemberExpression(m) = &call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &m.object else {
        return false;
    };
    if obj.name != "Object" || m.property.name != "assign" {
        return false;
    }
    let Some(first) = call.arguments.first().and_then(Argument::as_expression) else {
        return false;
    };
    match first {
        Expression::Identifier(target) => {
            target.name == "exports" || exported.contains(target.name.as_str())
        }
        // CommonJS `module.exports`.
        Expression::StaticMemberExpression(member) => {
            member.property.name == "exports"
                && matches!(&member.object, Expression::Identifier(o) if o.name == "module")
        }
        _ => false,
    }
}

/// Name of the identifier reached by peeling TypeScript casts (`as`,
/// `satisfies`, non-null `!`) off an expression. `None` once a non-cast,
/// non-identifier node is reached. Lets `export default Vue as unknown as T`
/// resolve to the `Vue` binding.
fn identifier_through_casts<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::TSAsExpression(cast) => identifier_through_casts(&cast.expression),
        Expression::TSSatisfiesExpression(cast) => identifier_through_casts(&cast.expression),
        Expression::TSNonNullExpression(cast) => identifier_through_casts(&cast.expression),
        Expression::ParenthesizedExpression(p) => identifier_through_casts(&p.expression),
        _ => None,
    }
}

/// Collect every local binding name this module exports — both re-exported
/// bindings (`export default Vue`, `export { styled }`) and inline-declared
/// exports (`export const arrayMethods = …`, `export function f() {}`,
/// `export class C {}`). A name in this set is part of the module's own public
/// contract, so a top-level call that builds or patches it is module
/// initialization observed by consumers only after assembly — not external
/// state. Distinct from `module_exported_local_bindings`, which intentionally
/// covers only re-exported bindings for the `Object.assign` exemption.
fn module_exported_bindings(program: &Program) -> HashSet<String> {
    let mut out = HashSet::new();
    for stmt in &program.body {
        match stmt {
            Statement::ExportDefaultDeclaration(export) => {
                // `export default Vue` and `export default Vue as unknown as T`
                // both name the same exported binding.
                let id = match &export.declaration {
                    ExportDefaultDeclarationKind::Identifier(id) => Some(id.name.as_str()),
                    ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                        func.id.as_ref().map(|id| id.name.as_str())
                    }
                    ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                        class.id.as_ref().map(|id| id.name.as_str())
                    }
                    ExportDefaultDeclarationKind::TSAsExpression(cast) => {
                        identifier_through_casts(&cast.expression)
                    }
                    ExportDefaultDeclarationKind::TSSatisfiesExpression(cast) => {
                        identifier_through_casts(&cast.expression)
                    }
                    ExportDefaultDeclarationKind::TSNonNullExpression(cast) => {
                        identifier_through_casts(&cast.expression)
                    }
                    _ => None,
                };
                if let Some(name) = id {
                    out.insert(name.to_string());
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                for spec in &export.specifiers {
                    if let Some(local) = spec.local.identifier_name() {
                        out.insert(local.to_string());
                    }
                }
                match &export.declaration {
                    Some(Declaration::FunctionDeclaration(func)) => {
                        if let Some(id) = &func.id {
                            out.insert(id.name.to_string());
                        }
                    }
                    Some(Declaration::ClassDeclaration(class)) => {
                        if let Some(id) = &class.id {
                            out.insert(id.name.to_string());
                        }
                    }
                    Some(Declaration::VariableDeclaration(decl)) => {
                        for declarator in &decl.declarations {
                            if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                                out.insert(id.name.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

/// True when `call` is a mixin-builder call that hands an exported binding to a
/// builder function: `f(Exported)` where `f` is a bare identifier and at least
/// one argument is a bare identifier the module exports (`initMixin(Vue)` where
/// the module ends with `export default Vue`). Pre-class-syntax frameworks
/// assemble a constructor at module scope by passing it through a chain of such
/// builders, each attaching methods to its prototype — equivalent to a series of
/// `class extends` mixins. The whole purpose of importing the module is to obtain
/// the fully assembled export, so the call is intentional initialization, not a
/// tree-shaking hazard. The exported-argument requirement keeps the exemption
/// narrow: a bare `init()` or `register(plugin)` whose argument is not an
/// exported binding is still flagged.
fn is_exported_builder_call(
    call: &oxc_ast::ast::CallExpression,
    exported: &HashSet<String>,
) -> bool {
    if !matches!(call.callee, Expression::Identifier(_)) {
        return false;
    }
    call.arguments.iter().any(|arg| {
        matches!(arg.as_expression(), Some(Expression::Identifier(id)) if exported.contains(id.name.as_str()))
    })
}

/// Root identifier of a callback statement that patches an exported object:
/// - `fn(Exported, …)` — a builder call whose first argument is an exported
///   binding (`def(arrayMethods, method, mutator)`); or
/// - `Exported[k] = v` / `Exported.k = v` — a member assignment whose target
///   roots at an exported binding.
///
/// Returns the exported root name if the statement patches an exported object,
/// `None` otherwise.
fn patched_export_root<'a>(stmt: &'a Statement<'a>) -> Option<&'a str> {
    let Statement::ExpressionStatement(es) = stmt else {
        return None;
    };
    match &es.expression {
        Expression::CallExpression(call) => {
            if !matches!(call.callee, Expression::Identifier(_)) {
                return None;
            }
            match call.arguments.first().and_then(Argument::as_expression) {
                Some(Expression::Identifier(id)) => Some(id.name.as_str()),
                _ => None,
            }
        }
        Expression::AssignmentExpression(assign) => assignment_target_root(&assign.left),
        _ => None,
    }
}

/// True when `call` is a prototype-patcher `forEach` that patches an exported
/// object: `localArray.forEach(item => def(Exported, item, …))`. The receiver is
/// a module-local `const` array, the single callback argument is a
/// function/arrow, and every effectful statement in its body patches the *same*
/// exported binding (see `patched_export_root`). Local `const`/`let`/`var`
/// declarations inside the body (e.g. `const original = arrayProto[method]`) are
/// ignored — they only stage values for the patch. This is the pre-class-syntax
/// pattern for intercepting prototype methods on an exported object: importing
/// the module exists to obtain the patched export, so the iteration is intended
/// initialization, not a tree-shaking hazard. A `forEach` whose body touches any
/// non-exported target, or whose receiver is not a module-local const, is still
/// flagged.
fn is_export_patching_foreach(
    call: &oxc_ast::ast::CallExpression,
    locals: &HashSet<String>,
    exported: &HashSet<String>,
) -> bool {
    let Expression::StaticMemberExpression(m) = &call.callee else {
        return false;
    };
    if m.property.name != "forEach" {
        return false;
    }
    let Expression::Identifier(receiver) = &m.object else {
        return false;
    };
    if !locals.contains(receiver.name.as_str()) {
        return false;
    }
    if call.arguments.len() != 1 {
        return false;
    }
    let body = match &call.arguments[0] {
        Argument::ArrowFunctionExpression(arrow) => &arrow.body,
        Argument::FunctionExpression(func) => match &func.body {
            Some(body) => body,
            None => return false,
        },
        _ => return false,
    };
    let mut patched: Option<&str> = None;
    for stmt in &body.statements {
        // Local staging declarations inside the callback are inert.
        if matches!(stmt, Statement::VariableDeclaration(_)) {
            continue;
        }
        let Some(root) = patched_export_root(stmt) else {
            return false;
        };
        if !exported.contains(root) {
            return false;
        }
        match patched {
            None => patched = Some(root),
            Some(name) if name == root => {}
            Some(_) => return false,
        }
    }
    patched.is_some()
}

/// Root identifier name of a member-access assignment target (`obj.k`,
/// `obj[k]`, `a.b.c`). Returns `None` for a bare identifier target (a plain
/// reassignment, which is not a local-state mutation).
fn assignment_target_root<'a>(target: &'a AssignmentTarget<'a>) -> Option<&'a str> {
    match target {
        AssignmentTarget::StaticMemberExpression(m) => member_object_root(&m.object),
        AssignmentTarget::ComputedMemberExpression(m) => member_object_root(&m.object),
        _ => None,
    }
}

/// Root identifier name of a member-access object chain (`a` in `a.b.c`).
fn member_object_root<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => member_object_root(&m.object),
        Expression::ComputedMemberExpression(m) => member_object_root(&m.object),
        _ => None,
    }
}

/// True when a value expression (an argument to `lookup.set(...)`, or the RHS
/// of `obj[k] = v`) is free of statements/effects that would make the
/// surrounding `forEach` genuinely impure: a bare free-function call
/// (`transform(x)`), a `new` expression, `await`, or `yield`. Method calls on
/// the iterated data (`name.toLowerCase()`) are pure value transformations and
/// remain allowed, so the exemption still covers the issue's example while a
/// callback that invokes an external function keeps firing.
fn is_pure_value_expr(expr: &Expression) -> bool {
    match expr {
        Expression::NewExpression(_)
        | Expression::AwaitExpression(_)
        | Expression::YieldExpression(_) => false,
        Expression::CallExpression(call) => {
            // A bare `fn(...)` callee is a free function call — impure. A
            // `obj.method(...)` callee is a value transformation — keep
            // walking its receiver and arguments.
            if matches!(call.callee, Expression::Identifier(_)) {
                return false;
            }
            is_pure_value_expr(&call.callee)
                && call.arguments.iter().all(is_pure_argument)
        }
        Expression::StaticMemberExpression(m) => is_pure_value_expr(&m.object),
        Expression::ComputedMemberExpression(m) => {
            is_pure_value_expr(&m.object) && is_pure_value_expr(&m.expression)
        }
        Expression::BinaryExpression(b) => {
            is_pure_value_expr(&b.left) && is_pure_value_expr(&b.right)
        }
        Expression::LogicalExpression(l) => {
            is_pure_value_expr(&l.left) && is_pure_value_expr(&l.right)
        }
        Expression::ConditionalExpression(c) => {
            is_pure_value_expr(&c.test)
                && is_pure_value_expr(&c.consequent)
                && is_pure_value_expr(&c.alternate)
        }
        Expression::ParenthesizedExpression(p) => is_pure_value_expr(&p.expression),
        Expression::UnaryExpression(u) => is_pure_value_expr(&u.argument),
        Expression::TemplateLiteral(t) => t.expressions.iter().all(is_pure_value_expr),
        Expression::ArrayExpression(_) | Expression::ObjectExpression(_) => false,
        Expression::TSAsExpression(a) => is_pure_value_expr(&a.expression),
        Expression::TSNonNullExpression(n) => is_pure_value_expr(&n.expression),
        Expression::TSSatisfiesExpression(s) => is_pure_value_expr(&s.expression),
        Expression::Identifier(_)
        | Expression::ThisExpression(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::StringLiteral(_) => true,
        _ => false,
    }
}

fn is_pure_argument(arg: &Argument) -> bool {
    match arg.as_expression() {
        Some(expr) => is_pure_value_expr(expr),
        None => false,
    }
}

/// True when a callback-body statement only mutates module-local state:
/// - `lookup.set(...)` / `lookup.add(...)` where `lookup` is a module-scoped
///   `const` (a `Map`/`Set` populated in place), with pure arguments; or
/// - `obj[k] = v` / `obj.k = v` where the assignment target roots at a
///   module-scoped `const`, with a pure right-hand side.
fn is_local_mutation_stmt(stmt: &Statement, locals: &HashSet<String>) -> bool {
    let Statement::ExpressionStatement(es) = stmt else {
        return false;
    };
    match &es.expression {
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(m) = &call.callee else {
                return false;
            };
            if !matches!(m.property.name.as_str(), "set" | "add") {
                return false;
            }
            let Expression::Identifier(obj) = &m.object else {
                return false;
            };
            locals.contains(obj.name.as_str())
                && call.arguments.iter().all(is_pure_argument)
        }
        Expression::AssignmentExpression(assign) => {
            let Some(root) = assignment_target_root(&assign.left) else {
                return false;
            };
            locals.contains(root) && is_pure_value_expr(&assign.right)
        }
        _ => false,
    }
}

/// True when `call` is a module-level data-initialization `forEach` whose only
/// effect is populating a module-scoped `const` lookup:
/// `localArray.forEach(item => localLookup.set(item.k, item.v))`. The receiver
/// is a module-scoped `const`, the single callback argument is a function/arrow,
/// and every statement in its body is a local mutation (see
/// `is_local_mutation_stmt`). Any other statement, an external call, I/O,
/// `throw`, or a non-local receiver keeps the rule firing.
fn is_data_init_foreach(
    call: &oxc_ast::ast::CallExpression,
    locals: &HashSet<String>,
) -> bool {
    let Expression::StaticMemberExpression(m) = &call.callee else {
        return false;
    };
    if m.property.name != "forEach" {
        return false;
    }
    let Expression::Identifier(receiver) = &m.object else {
        return false;
    };
    if !locals.contains(receiver.name.as_str()) {
        return false;
    }
    if call.arguments.len() != 1 {
        return false;
    }
    let body = match &call.arguments[0] {
        Argument::ArrowFunctionExpression(arrow) => &arrow.body,
        Argument::FunctionExpression(func) => match &func.body {
            Some(body) => body,
            None => return false,
        },
        _ => return false,
    };
    if body.statements.is_empty() {
        return false;
    }
    body.statements
        .iter()
        .all(|stmt| is_local_mutation_stmt(stmt, locals))
}

/// True when the program is a library's public entry barrel by content shape:
/// it has at least one `export *` re-export (`export * from "./schemas.js"`,
/// `export * as ns from "..."`) and those star re-exports outnumber its
/// top-level effectful call/`new` statements. Such a module's whole purpose is
/// to surface the package's full API through one subpath export — it is imported
/// to obtain that API, never as a tree-shaking target — so the small number of
/// top-level registration calls that accompany the re-exports (e.g. zod's
/// `config(en())` registering the default English locale at module load) are
/// intentional initialization, not a tree-shaking hazard.
///
/// Requiring the star re-exports to dominate keeps the exemption tied to the
/// barrel shape: a module with real logic (only a handful of `export *` lines
/// among many top-level side-effecting statements) is not dominated by
/// re-exports and stays flagged, and a module with no `export *` re-export at
/// all is never a barrel.
fn is_entry_barrel_shape(program: &Program) -> bool {
    let mut star_reexports = 0usize;
    let mut effectful_calls = 0usize;
    for stmt in &program.body {
        match stmt {
            Statement::ExportAllDeclaration(_) => star_reexports += 1,
            Statement::ExpressionStatement(es)
                if effectful_expression_label(&es.expression).is_some() =>
            {
                effectful_calls += 1;
            }
            _ => {}
        }
    }
    star_reexports > 0 && star_reexports > effectful_calls
}

fn effectful_expression_label(expr: &Expression) -> Option<&'static str> {
    match expr {
        Expression::CallExpression(_) => Some("call"),
        Expression::NewExpression(_) => Some("`new` expression"),
        _ => None,
    }
}

fn has_pure_annotation(source: &str, span_start: usize) -> bool {
    // Look backwards from the statement start for a PURE comment.
    let before = &source[..span_start];
    let trimmed = before.trim_end();
    trimmed.ends_with("*/")
        && (trimmed.contains("#__PURE__") || trimmed.contains("@__PURE__"))
        && {
            // The comment must be the immediately preceding token.
            if let Some(comment_start) = trimmed.rfind("/*") {
                let comment = &trimmed[comment_start..];
                comment.contains("#__PURE__") || comment.contains("@__PURE__")
            } else {
                false
            }
        }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir || is_test_file(ctx.path) {
            return Vec::new();
        }

        let program = semantic.nodes().program();

        if is_test_setup_path(ctx.path)
            || is_cli_entry(ctx.path, ctx.source)
            || is_benchmark_or_profile_script(ctx.path)
            || shape_is_test_setup(program)
            || is_vitest_bench_shape(program)
            || is_vanilla_extract_style_file(program)
            || is_server_entry_shape(program)
            || is_cli_main_entry_shape(program)
            || is_react_entry_shape(program)
            || is_solid_entry_shape(program)
            || is_vue_entry_shape(program)
            || is_preact_entry_shape(program)
            || is_angular_entry_shape(program)
            || is_gulp_task_file(program)
            || is_storybook_addon_file(program)
            || is_mcp_server_file(program)
            || is_data_generation_script(program)
            || is_entry_barrel_shape(program)
        {
            return Vec::new();
        }

        if is_framework_entry_point(ctx.path, ctx.project)
            || is_tanstack_start_entry(ctx.path, ctx.project)
            || ctx.project.is_script_entry_file(ctx.path)
            || ctx
                .project
                .project_root
                .as_deref()
                .is_some_and(|root| is_top_level_script_dir_path(ctx.path, root))
            || is_config_file(ctx.path)
            || is_browser_asset_dir_path(ctx.path)
        {
            return Vec::new();
        }

        let start_transition_names = react_start_transition_bindings(program);
        let vue_reactivity_names = vue_reactivity_bindings(program);
        let module_locals = module_const_bindings(program);
        let new_locals = module_const_new_bindings(program);
        let exported_locals = module_exported_local_bindings(program);
        let exported_bindings = module_exported_bindings(program);

        let mut diagnostics = Vec::new();
        for stmt in &program.body {
            let Statement::ExpressionStatement(expr_stmt) = stmt else { continue };
            let Some(label) = effectful_expression_label(&expr_stmt.expression) else {
                continue;
            };

            if let Expression::CallExpression(call) = &expr_stmt.expression
                && (is_start_transition_call(call, &start_transition_names)
                    || is_vue_reactivity_setup_call(call, &vue_reactivity_names)
                    || is_process_signal_handler_call(call)
                    || is_library_plugin_registration_call(call)
                    || is_commander_subcommand_chain(call)
                    || is_data_init_foreach(call, &module_locals)
                    || is_local_const_config_call(call, &new_locals)
                    || is_export_object_assign(call, &exported_locals)
                    || is_exported_builder_call(call, &exported_bindings)
                    || is_export_patching_foreach(call, &module_locals, &exported_bindings))
            {
                continue;
            }

            if has_pure_annotation(ctx.source, expr_stmt.span.start as usize) {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, expr_stmt.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Top-level {label} executes on import and blocks tree-shaking. \
                     Move it into a function, or mark it `/*#__PURE__*/` if truly side-effect-free."
                ),
                severity: Severity::Warning,
                span: None,
            });
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
    

    #[test]
    fn flags_top_level_bare_call() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "doThing();", "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_top_level_new_expression() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "new EventEmitter();", "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_pure_annotated_call() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "/*#__PURE__*/ registerSomething();", "t.ts");
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_test_files() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "expectType<string>(foo());", "main.test-d.ts");
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_cypress_e2e_file_by_extension() {
        // Issue #1868: Cypress E2E specs use the `.cy.*` extension and always
        // open with a top-level `describe(...)` — required Cypress API. These
        // files are loaded by the Cypress runner, never imported as modules, so
        // tree-shaking does not apply.
        let src = "describe('manual register form validation', () => {\n\
                       it('should validate the form', () => {\n\
                           cy.visit('http://localhost:3000/manual-register-form');\n\
                       });\n\
                   });";
        for path in [
            "cypress/e2e/manualRegisterForm.cy.ts",
            "cypress/e2e/manualRegisterForm.cy.js",
            "cypress/e2e/manualRegisterForm.cy.tsx",
            "cypress/e2e/manualRegisterForm.cy.jsx",
        ] {
            let diags = crate::rules::test_helpers::run_rule(&Check, src, path);
            assert!(diags.is_empty(), "{path} should be exempt, got {diags:?}");
        }
        // The `.cy.` infix is what grants the exemption: the same top-level
        // call in a genuine production module still flags.
        let prod = crate::rules::test_helpers::run_rule(&Check, "describe('x', () => {});", "src/index.ts");
        assert_eq!(prod.len(), 1, "production src/index.ts must still flag");
    }

    #[test]
    fn skips_vitest_unit_file_by_extension() {
        // Issue #2233: Qwik (and other Vitest projects) name unit tests with the
        // `.unit.*` infix. These files contain only top-level `test(...)` /
        // `describe(...)` registration calls — the Vitest runner API — and are
        // executed by the runner, never imported as modules, so tree-shaking
        // does not apply.
        let src = "import { assert, test } from 'vitest';\n\
                   import { scopeStylesheet } from './scoped-stylesheet';\n\
                   test('selectors', () => {\n\
                       assert.equal(scopeStylesheet('div {}', '_'), 'div.x_ {}');\n\
                   });\n\
                   test('unicode', () => {\n\
                       assert.equal(scopeStylesheet('.a{}', '_'), '.a.x_{}');\n\
                   });";
        for path in [
            "packages/qwik/src/core/style/scoped-stylesheet.unit.ts",
            "packages/qwik/src/core/render/ssr/render-ssr.unit.tsx",
            "src/util/helper.unit.js",
            "src/util/helper.unit.jsx",
        ] {
            let diags = crate::rules::test_helpers::run_rule(&Check, src, path);
            assert!(diags.is_empty(), "{path} should be exempt, got {diags:?}");
        }
        // The `.unit.` infix is what grants the exemption: the same top-level
        // call in a genuine production module still flags.
        let prod = crate::rules::test_helpers::run_rule(&Check, "test('x', () => {});", "src/index.ts");
        assert_eq!(prod.len(), 1, "production src/index.ts must still flag");
    }

    #[test]
    fn skips_jasmine_underscore_spec_file() {
        // Issue #1737: Jasmine/Angular test files use the `_spec.ts` underscore
        // naming convention (e.g. `recorder_spec.ts`). The top-level `describe(...)`
        // is the test-runner API, loaded by the runner, never imported as a module.
        let src = "\
            import { normalize } from '@angular-devkit/core';\n\
            import { SimpleFileEntry } from './entry';\n\
            import { UpdateRecorderBase } from './recorder';\n\
            describe('UpdateRecorderBase', () => {\n\
                it('works for simple files', () => {\n\
                    const buffer = Buffer.from('Hello World');\n\
                });\n\
            });\n";
        for path in [
            "packages/angular_devkit/schematics/src/tree/recorder_spec.ts",
            "src/memoize_spec.tsx",
            "src/legacy_spec.js",
        ] {
            let diags = crate::rules::test_helpers::run_rule(&Check, src, path);
            assert!(diags.is_empty(), "{path} should be exempt, got {diags:?}");
        }
        // The `_spec.` infix is what grants the exemption: the same top-level
        // call in a genuine production module still flags.
        let prod = crate::rules::test_helpers::run_rule(&Check, "describe('x', () => {});", "src/index.ts");
        assert_eq!(prod.len(), 1, "production src/index.ts must still flag");
    }

    #[test]
    fn skips_nock_mock_fixture_file() {
        // Issue #1698: `*.mock.ts` files are HTTP mocking fixtures loaded by the
        // test runner. Their whole purpose is to register `nock(...)` HTTP
        // interceptors at module scope, so their top-level side effects are
        // mandatory and deliberate — never bundled for production.
        let src = "\
            import nock from 'nock';\n\
            nock('http://localhost:1337', { encodedQueryParams: true })\n\
                .post('/api/posts', { data: { title: 'foo' } })\n\
                .reply(200, { id: 1 });\n\
            nock('http://localhost:1337', { encodedQueryParams: true })\n\
                .get('/api/posts')\n\
                .reply(200, []);\n";
        for path in [
            "packages/rest/src/data-providers/strapi-v4/specs/index.mock.ts",
            "src/handlers/posts.mock.js",
            "src/handlers/posts.mock.tsx",
        ] {
            let diags = crate::rules::test_helpers::run_rule(&Check, src, path);
            assert!(diags.is_empty(), "{path} should be exempt, got {diags:?}");
        }
        // The `.mock.` infix is what grants the exemption: the same top-level
        // calls in a genuine production module are still flagged.
        let prod = crate::rules::test_helpers::run_rule(&Check, src, "src/data-providers/strapi-v4/index.ts");
        assert_eq!(prod.len(), 2, "production src/*.ts must still flag both nock calls");
    }

    #[test]
    fn skips_vitest_bench_file_by_extension() {
        // Issue #2207: Vitest benchmark files use the `.bench.*` extension and
        // register suites with top-level `describe(...)`/`bench(...)` calls. They
        // are executed directly by `vitest bench`, never imported as modules, so
        // tree-shaking does not apply. The top-level `let benchName = …` init is
        // covered too since the whole file is exempt by extension.
        let src = "\
            import { bench, describe } from 'vitest';\n\
            import { RoutePattern } from '@remix-run/route-pattern';\n\
            let benchName = getBenchName();\n\
            describe('RoutePattern.href() — static path', () => {\n\
                bench(benchName, () => { pattern.href({}); });\n\
            });\n\
            describe('RoutePattern.href() — single param', () => {\n\
                bench(benchName, () => { pattern.href({ id: '42' }); });\n\
            });\n";
        for path in [
            "src/href.bench.ts",
            "src/href.bench.tsx",
            "src/href.bench.js",
            "src/href.bench.jsx",
        ] {
            let diags = crate::rules::test_helpers::run_rule(&Check, src, path);
            assert!(diags.is_empty(), "{path} should be exempt, got {diags:?}");
        }
        // The `.bench.` infix is what grants the extension exemption: the same
        // top-level calls in a genuine production module that does NOT import
        // from `vitest` are still flagged (the content-shape gate also requires
        // the import, so we drop it here to isolate the extension signal).
        let prod_src = "\
            describe('RoutePattern.href() — static path', () => {});\n\
            describe('RoutePattern.href() — single param', () => {});\n";
        let prod = crate::rules::test_helpers::run_rule(&Check, prod_src, "src/href.ts");
        assert_eq!(prod.len(), 2, "production src/href.ts must still flag both describe calls");
    }

    #[test]
    fn skips_vitest_bench_file_by_content_shape() {
        // Issue #2207: a Vitest benchmark file that does not use the `.bench.`
        // extension is still recognized by content shape — a `vitest` import plus
        // top-level `describe(...)`/`bench(...)` registration calls.
        let src = "\
            import { bench, describe } from 'vitest';\n\
            describe('Raw Parsing - Landing Page', () => {\n\
                bench('custom parser', () => { customParse(landingPage); });\n\
            });\n\
            describe('Raw Parsing - Blog Post', () => {\n\
                bench('custom parser', () => { customParse(blogPost); });\n\
            });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/html-parser.ts");
        assert!(diags.is_empty(), "vitest bench shape should be exempt, got {diags:?}");
    }

    #[test]
    fn still_flags_describe_without_vitest_import() {
        // Negative-space guard: a top-level `describe(...)` in a non-bench `.ts`
        // module WITHOUT a `vitest` import is not a benchmark registration — the
        // content-shape exemption requires the import gate — so it is still flagged.
        let src = "describe('x', () => {});";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/index.ts");
        assert_eq!(diags.len(), 1, "describe without a vitest import must still flag");
    }

    #[test]
    fn still_flags_real_side_effect_in_bench_shape() {
        // Negative-space guard: a genuine top-level side effect alongside the
        // registrations breaks the uniform bench shape, so the file is still
        // flagged. The `.bench.` extension is the unconditional escape hatch; a
        // plain module must earn the exemption with a pure registration body.
        let src = "\
            import { bench, describe } from 'vitest';\n\
            initSentry();\n\
            describe('x', () => { bench('y', () => {}); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/perf.ts");
        assert!(!diags.is_empty(), "a real top-level side effect must still flag, got {diags:?}");
    }

    #[test]
    fn skips_vanilla_extract_style_file() {
        // Issue #2183: vanilla-extract is a zero-runtime CSS-in-TS library. Style
        // modules call `globalStyle(...)`/`style(...)` at the top level — these are
        // build-time operations compiled into static CSS by the vanilla-extract
        // plugin, never bundled into runtime JS, so tree-shaking does not apply.
        let src = "\
            import { globalStyle } from '@vanilla-extract/css';\n\
            import { vars } from './theme-contract.css';\n\
            globalStyle('html, body', { color: vars.colors.black });\n\
            globalStyle('*, *:before, *:after', { boxSizing: 'border-box' });\n";
        for path in [
            "docs/app/styles/global.css.ts",
            "docs/app/components/Code/Pre.css.ts",
            "src/theme.css.js",
        ] {
            let diags = crate::rules::test_helpers::run_rule(&Check, src, path);
            assert!(diags.is_empty(), "{path} should be exempt, got {diags:?}");
        }
    }

    #[test]
    fn skips_vanilla_extract_recipes_subpackage() {
        // The exemption keys on any `@vanilla-extract/*` package, not only
        // `@vanilla-extract/css` (e.g. `@vanilla-extract/recipes`).
        let src = "\
            import { recipe } from '@vanilla-extract/recipes';\n\
            export const button = recipe({ base: { padding: 12 } });\n\
            recipe({ base: { padding: 4 } });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/button.css.ts");
        assert!(diags.is_empty(), "vanilla-extract recipes module should be exempt, got {diags:?}");
    }

    #[test]
    fn still_flags_css_ts_without_vanilla_extract_import() {
        // Negative-space guard: the `@vanilla-extract/*` import is the definitive
        // signal. A `.css.ts` file that does NOT import the vanilla-extract API is
        // not a vanilla-extract module, so a genuine top-level side effect in it is
        // still flagged.
        let src = "initSentry();";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/styles.css.ts");
        assert_eq!(diags.len(), 1, ".css.ts without a vanilla-extract import must still flag");
    }

    #[test]
    fn still_flags_real_side_effect_without_vanilla_extract_import() {
        // Negative-space guard: a genuine top-level side effect in a normal `.ts`
        // module with no vanilla-extract import is still flagged.
        let src = "initSentry();\nconsole.log('boot');";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/index.ts");
        assert_eq!(diags.len(), 2, "real side effects without a vanilla-extract import must still flag");
    }

    // --- (a) Vitest setup file exemption ----------------------------------

    #[test]
    fn allows_vitest_setup_file_by_convention_path() {
        let src = "\
            import { beforeAll, afterEach } from 'vitest';\n\
            beforeAll(() => { startMockServer({ onUnhandledRequest: 'error' }); });\n\
            afterEach(() => { mswServer.resetHandlers(); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/test-helpers/setup-msw.ts");
        assert!(
            diags.is_empty(),
            "vitest setup file by convention path should be exempt, got {diags:?}"
        );
    }

    // Regression for #288: a runner setup file's top-level side effect IS its
    // contract — the runner imports it to run exactly those effects.
    #[test]
    fn allows_vitest_setup_file_at_root() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "ensureWorkerDatabase();", "vitest.setup.ts");
        assert!(diags.is_empty(), "vitest.setup.ts should be exempt, got {diags:?}");
    }

    #[test]
    fn allows_jest_setup_file() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "installMatchers();", "jest.setup.ts");
        assert!(diags.is_empty(), "jest.setup.ts should be exempt, got {diags:?}");
    }

    #[test]
    fn allows_bare_setup_file() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "provisionDb();", "test/setup.ts");
        assert!(diags.is_empty(), "setup.ts should be exempt, got {diags:?}");
    }

    #[test]
    fn still_flags_regular_module_with_setup_in_name() {
        // `setupRouter.ts` is an ordinary module, not a runner setup file.
        let diags = crate::rules::test_helpers::run_rule(&Check, "buildRouter();", "src/setupRouter.ts");
        assert_eq!(diags.len(), 1, "setupRouter.ts must still be flagged, got {diags:?}");
    }

    #[test]
    fn allows_setup_tests_file() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "installMatchers();", "src/setupTests.ts");
        assert!(diags.is_empty(), "setupTests.ts should be exempt, got {diags:?}");
    }

    // Regression for #1419: Cypress support files run top-level registrations
    // by contract — the runner loads them before each test run for exactly
    // those side effects.
    #[test]
    fn allows_cypress_support_file() {
        let src = "\
            import { addQwikLoader } from 'cypress-ct-qwik';\n\
            addQwikLoader();\n\
            import './commands';\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "starters/features/cypress/cypress/support/component.ts");
        assert!(
            diags.is_empty(),
            "cypress/support/ files are test-runner entry points, got {diags:?}"
        );
    }

    // Regression for #1670: Playwright component-testing setup files
    // (`playwright/index.{ts,js}`) are loaded by the runner for their top-level
    // fixture registrations — the Playwright analogue of Cypress support files.
    #[test]
    fn allows_playwright_component_setup_file() {
        let src = "\
            import { setProjectAnnotations } from '@storybook/react-vite';\n\
            import sbAnnotations from '../.storybook/preview';\n\
            setProjectAnnotations([sbAnnotations]);\n";
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            src,
            "test-storybooks/portable-stories-kitchen-sink/react/playwright/index.ts",
        );
        assert!(
            diags.is_empty(),
            "playwright/index.ts is a Playwright component-testing setup file, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_index_file_outside_playwright_dir() {
        // `index.ts` is only exempt under a `playwright/` directory, not blanket.
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            "setProjectAnnotations([sbAnnotations]);",
            "src/foo.ts",
        );
        assert_eq!(
            diags.len(),
            1,
            "a top-level side-effect call in a non-setup file must still fire, got {diags:?}"
        );
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            "registerSideEffect();",
            "src/index.ts",
        );
        assert_eq!(
            diags.len(),
            1,
            "index.ts outside playwright/ must not be blanket-exempted, got {diags:?}"
        );
    }

    #[test]
    fn allows_vitest_setup_file_by_content_shape() {
        let src = "\
            import { beforeAll, afterEach } from 'vitest';\n\
            beforeAll(() => { boot(); });\n\
            afterEach(() => { reset(); });\n\
            expect.extend({ toBeFoo() { return { pass: true, message: () => '' }; } });\n";
        // Path does NOT match the convention — content shape carries the exemption.
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/some/random/file.ts");
        assert!(
            diags.is_empty(),
            "all-hooks content shape should exempt the file, got {diags:?}"
        );
    }

    #[test]
    fn flags_top_level_beforeAll_without_vitest_import() {
        // `beforeAll` defined locally — it is a top-level value binding, so the
        // shape check treats it as a user function, not an injected hook.
        let src = "\
            function beforeAll(fn: () => void) { fn(); }\n\
            beforeAll(() => someSideEffect());\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/foo.ts");
        assert_eq!(
            diags.len(),
            1,
            "beforeAll without vitest import must be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_when_content_mixes_hooks_with_other_calls() {
        let src = "\
            beforeAll(() => { boot(); });\n\
            someOtherCall();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/some/file.ts");
        assert_eq!(
            diags.len(),
            2,
            "non-hook call breaks the setup-file shape, both stmts flagged"
        );
    }

    // Regression for #1903: a Jest setup file uses the global `beforeEach` /
    // `afterEach` hooks (injected by Jest, no import) at the top level. The
    // content shape is a pure setup file, so it must be exempt even without a
    // `"vitest"` import and even on a path the convention check misses
    // (`test-utils/setupTestFramework.ts`).
    #[test]
    fn allows_jest_setup_file_with_global_hooks_no_import() {
        let src = "\
            import { resetWarnOnce } from '../src/utils/warnOnce';\n\
            beforeEach(() => { console.error = jest.fn(); resetWarnOnce(); });\n\
            afterEach(() => { console.error = consoleError; });\n";
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            src,
            "packages/styled-components/test-utils/setupTestFramework.ts",
        );
        assert!(
            diags.is_empty(),
            "Jest setup file using global hooks (no vitest import) should be exempt, got {diags:?}"
        );
    }

    // --- (b) Framework entry point exemption ------------------------------

    #[test]
    fn allows_tanstack_start_client_entry() {
        // `client.tsx` at any depth is a TanStack Start entry point.
        let src = "\
            import { startTransition } from 'react';\n\
            import { hydrateRoot } from 'react-dom/client';\n\
            initZodLocale();\n\
            stripSensitiveQueryFromUrlBar();\n\
            startTransition(() => { hydrateRoot(document, <StartClient />); });\n";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "src/app/client.tsx", &crate::project::ProjectCtx::for_test_with_framework("tanstack-router"), crate::rules::file_ctx::default_static_file_ctx());
        assert!(
            diags.is_empty(),
            "framework entry point should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_tanstack_router_entry() {
        let src = "createRouter({ routeTree, defaultPreload: 'intent' });\n";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "src/app/router.tsx", &crate::project::ProjectCtx::for_test_with_framework("tanstack-router"), crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty(), "router.tsx entry should be exempt");
    }

    #[test]
    fn flags_client_tsx_outside_app_dir() {
        // Same pattern as the entry — but the file lives outside app/, so NOT exempt.
        let src = "\
            import { startTransition } from 'react';\n\
            import { hydrateRoot } from 'react-dom/client';\n\
            initZodLocale();\n";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "src/utils/client.tsx", &crate::project::ProjectCtx::for_test_with_framework("tanstack-router"), crate::rules::file_ctx::default_static_file_ctx());
        assert_eq!(
            diags.len(),
            1,
            "client.tsx outside app/ must still be flagged, got {diags:?}"
        );
    }

    // --- static browser assets (public/static/assets) --------------------

    // Regression for #1752: a vanilla `<script>`-loaded browser script in
    // `public/` is served verbatim and never bundled, so its top-level
    // imperative body is intentional, not a tree-shaking hazard.
    #[test]
    fn allows_browser_script_in_public_dir() {
        let src = "\
            Array.from(document.getElementsByTagName(\"pre\")).forEach((element) => {\n\
                element.setAttribute(\"tabindex\", \"0\");\n\
            });\n";
        for path in [
            "www/public/makeScrollableCodeFocusable.js",
            "static/analytics.js",
            "src/assets/widget.js",
        ] {
            let diags = crate::rules::test_helpers::run_rule(&Check, src, path);
            assert!(
                diags.is_empty(),
                "{path} is a static browser asset and should be exempt, got {diags:?}"
            );
        }
    }

    #[test]
    fn still_flags_module_with_public_substring_segment() {
        // `publicApi/` is not the `public/` asset directory — segment, not
        // substring, matching keeps an ordinary library module flagged.
        let diags =
            crate::rules::test_helpers::run_rule(&Check, "registerWidget();", "src/publicApi/index.ts");
        assert_eq!(
            diags.len(),
            1,
            "src/publicApi/ is not a static-asset dir and must still flag, got {diags:?}"
        );
    }

    // --- (c) `startTransition` from "react" -------------------------------

    #[test]
    fn allows_start_transition_from_react() {
        let src = "\
            import { startTransition } from 'react';\n\
            startTransition(() => { hydrateRoot(document, null); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert!(
            diags.is_empty(),
            "startTransition imported from react is exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_aliased_start_transition_from_react() {
        let src = "\
            import { startTransition as ST } from 'react';\n\
            ST(() => { hydrateRoot(document, null); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert!(
            diags.is_empty(),
            "aliased startTransition import is exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_start_transition_from_other_source() {
        let src = "\
            import { startTransition } from 'some-other-lib';\n\
            startTransition(() => { boot(); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(
            diags.len(),
            1,
            "startTransition not from react is still flagged"
        );
    }

    #[test]
    fn still_flags_bare_start_transition_identifier_without_import() {
        let src = "startTransition(() => { boot(); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(
            diags.len(),
            1,
            "no import binding means no exemption"
        );
    }

    // --- (d) Config files, dtslint/, test-d/ exemptions (Closes #807) --------

    #[test]
    fn allows_config_file_with_side_effects() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "setEnvVariablesThatAreUsedBeforeSetup();", "vitest.config.mts");
        assert!(diags.is_empty(), "config files should be exempt, got {diags:?}");
    }

    #[test]
    fn allows_dtslint_type_check_file() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "foo(bar, baz)(qux);", "dtslint/Traversable.ts");
        assert!(diags.is_empty(), "dtslint/ files are type-checking utilities, got {diags:?}");
    }

    #[test]
    fn allows_test_d_type_test_file() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "expectNotAssignable(foo);", "test-d/schema.ts");
        assert!(diags.is_empty(), "test-d/ files are tsd type-testing utilities, got {diags:?}");
    }

    // --- (e) Server application entry points (Closes #1113) ----------------

    // Regression for #1113: a Fastify server entry point registers routes,
    // hooks, and content-type parsers at the top level by contract, then
    // starts the server with `fastify.listen()`. None of it is tree-shakeable.
    #[test]
    fn allows_fastify_server_entry_point() {
        let src = "\
            const fastify = Fastify({ exposeHeadRoutes: false, bodyLimit: MAX_FILE_SIZE });\n\
            fastify.addContentTypeParser('application/octet-stream', { parseAs: 'buffer' }, (_req, body, done) => done(null, body));\n\
            fastify.addHook('preHandler', authenticateTeamId);\n\
            fastify.get('/v8/artifacts/status', async (_req, reply) => reply.send({ status: 'enabled' }));\n\
            fastify.listen({ port: 3000 });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/index.ts");
        assert!(
            diags.is_empty(),
            "a module that starts a server with fastify.listen() is an entry point, got {diags:?}"
        );
    }

    #[test]
    fn allows_express_server_entry_point() {
        let src = "\
            const app = express();\n\
            app.use(cors());\n\
            app.get('/health', (_req, res) => res.send('ok'));\n\
            app.listen(8080);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "server.ts");
        assert!(diags.is_empty(), "express app.listen() entry point is exempt, got {diags:?}");
    }

    #[test]
    fn allows_bare_listen_server_entry_point() {
        let src = "\
            registerRoutes(server);\n\
            listen(server, 3000);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/main.ts");
        assert!(diags.is_empty(), "a bare listen() call marks a server entry point, got {diags:?}");
    }

    #[test]
    fn still_flags_library_module_without_listen() {
        // No top-level `listen` call — an ordinary library module whose
        // top-level effects DO block tree-shaking.
        let src = "\
            register('widget');\n\
            doSideEffect();\n\
            new EventEmitter();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/util.ts");
        assert_eq!(
            diags.len(),
            3,
            "library module without listen() must still be flagged, got {diags:?}"
        );
    }

    // Regression for #1592: real-world server entry points branch their listen
    // call on configuration (socket activation vs. normal startup), so the
    // `listen()` lives inside a top-level `if`/`else`. The server-entry detection
    // must look one level into the branches; the surrounding request- and
    // signal-handler registrations are mandatory side effects, not library code.
    #[test]
    fn allows_socket_activation_conditional_server_entry_point() {
        let src = "\
            const socket_activation = listen_pid === process.pid && listen_fds === 1;\n\
            const server = polka({ server: httpServer }).use(handler);\n\
            if (socket_activation) {\n\
                server.listen({ fd: SD_LISTEN_FDS_START }, () => {});\n\
            } else {\n\
                server.listen({ path, host, port }, () => {});\n\
            }\n\
            httpServer.on('request', (req) => {});\n\
            process.on('SIGTERM', graceful_shutdown);\n\
            process.on('SIGINT', graceful_shutdown);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/index.js");
        assert!(
            diags.is_empty(),
            "a server entry whose listen() is inside an if/else must be exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_non_listen_side_effect_inside_conditional() {
        // Negative space: a genuine top-level side effect inside an `if` block is
        // NOT a server `listen()`, so the module is not a server entry and the
        // surrounding effects must still be flagged. Only the recognized listen
        // shape grants the exemption.
        let src = "\
            if (featureEnabled) {\n\
                registerWidget('toolbar');\n\
            }\n\
            doSideEffect();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "a non-listen side effect outside a server entry must still flag, got {diags:?}"
        );
    }

    // Regression for #1651: the idiomatic Node.js startup pattern chains error
    // handling onto the listen promise — `app.listen({port}).catch(console.error)`
    // — so the top-level expression is a `.catch()` member call whose object is
    // the `listen` call. The server-entry detection must unwrap the continuation.
    #[test]
    fn allows_listen_catch_chained_server_entry_point() {
        let src = "\
            const app = fastify({ logger: true });\n\
            app.get('/', schema, async (req, reply) => ({ hello: 'world' }));\n\
            app.listen({ port: 3000 }).catch(console.error);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "examples/simple.mjs");
        assert!(
            diags.is_empty(),
            "app.listen().catch() is a server entry point startup pattern, got {diags:?}"
        );
    }

    #[test]
    fn allows_listen_then_chained_server_entry_point() {
        let src = "\
            app.register(routes);\n\
            app.listen({ port: 3000 }).then(() => log('up')).catch(console.error);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "server.mjs");
        assert!(
            diags.is_empty(),
            "app.listen().then().catch() is a server entry point startup pattern, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_non_listen_catch_chain() {
        // A `.catch()` continuation on a non-`listen` call is an ordinary
        // top-level side effect: unwrapping the chain must not exempt it.
        let src = "\
            register('widget');\n\
            startSomething().catch(console.error);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/util.ts");
        assert_eq!(
            diags.len(),
            2,
            "a non-listen .catch() chain must still be flagged, got {diags:?}"
        );
    }

    // Regression for #2125: Deno Edge Functions start their HTTP server with a
    // top-level `Deno.serve(...)` (modern API) — the runtime equivalent of
    // `app.listen()`. The whole module is one server-startup call and is never
    // tree-shaken.
    #[test]
    fn allows_deno_serve_server_entry_point() {
        let src = "\
            import 'https://deno.land/x/xhr@0.3.0/mod.ts';\n\
            Deno.serve(async (req) => {\n\
                const { query } = await req.json();\n\
                return new Response('ok');\n\
            });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "functions/openai/index.ts");
        assert!(diags.is_empty(), "Deno.serve() is a server entry point, got {diags:?}");
    }

    // Regression for #2125: the Bun runtime server entry point is
    // `Bun.serve({ fetch })`, the runtime equivalent of `app.listen()`.
    #[test]
    fn allows_bun_serve_server_entry_point() {
        let src = "\
            Bun.serve({\n\
                fetch(req) {\n\
                    return new Response('ok');\n\
                },\n\
            });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/server.ts");
        assert!(diags.is_empty(), "Bun.serve() is a server entry point, got {diags:?}");
    }

    // Regression for #2125: legacy Deno Edge Functions use the bare `serve(...)`
    // entry from the Deno std HTTP library — the canonical Deno server startup,
    // matching the bare-`listen()` precedent.
    #[test]
    fn allows_bare_serve_server_entry_point() {
        let src = "\
            import { serve } from 'https://deno.land/std@0.170.0/http/server.ts';\n\
            import { handler } from './handler.tsx';\n\
            serve(handler);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "functions/og-images/index.ts");
        assert!(diags.is_empty(), "bare serve() is a Deno server entry point, got {diags:?}");
    }

    #[test]
    fn still_flags_non_deno_member_serve_call() {
        // Negative space: `.serve(...)` on an arbitrary object is not the
        // Deno/Bun runtime entry shape (only `Deno.serve`/`Bun.serve` are), so an
        // ordinary `foo.serve()` side effect must still be flagged.
        let src = "\
            register('widget');\n\
            foo.serve();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/util.ts");
        assert_eq!(
            diags.len(),
            2,
            "an arbitrary obj.serve() is not a server entry and must still flag, got {diags:?}"
        );
    }

    // --- (f) React application entry points (Closes #1429) -----------------

    // Regression for #1429: the canonical React 18 entry point mounts the app
    // at module level with `ReactDOM.createRoot(...).render(...)`. That is the
    // entry file's whole purpose and it is never imported, so it must not be
    // flagged.
    #[test]
    fn allows_react_create_root_entry_point() {
        let src = "\
            import * as React from 'react';\n\
            import * as ReactDOM from 'react-dom/client';\n\
            import App from './App.tsx';\n\
            ReactDOM.createRoot(document.getElementById('root')!).render(\n\
              <React.StrictMode>\n\
                <App />\n\
              </React.StrictMode>,\n\
            );\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/main.tsx");
        assert!(
            diags.is_empty(),
            "ReactDOM.createRoot().render() entry point is exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_bare_create_root_render_entry_point() {
        let src = "\
            import { createRoot } from 'react-dom/client';\n\
            createRoot(document.getElementById('root')!).render(<App />);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/index.tsx");
        assert!(
            diags.is_empty(),
            "createRoot().render() entry point is exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_hydrate_root_entry_point() {
        let src = "\
            import { hydrateRoot } from 'react-dom/client';\n\
            hydrateRoot(document, <App />);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "entry.client.tsx");
        assert!(
            diags.is_empty(),
            "hydrateRoot() entry point is exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_legacy_react_dom_render_entry_point() {
        let src = "\
            import ReactDOM from 'react-dom';\n\
            ReactDOM.render(<App />, document.getElementById('root'));\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/index.tsx");
        assert!(
            diags.is_empty(),
            "legacy ReactDOM.render() entry point is exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_unrelated_render_method_call() {
        // A bare `.render()` on some object is NOT a React bootstrap; an
        // ordinary module's top-level side effect still blocks tree-shaking.
        let src = "\
            template.render(data);\n\
            doSideEffect();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget.ts");
        assert_eq!(
            diags.len(),
            2,
            "unrelated .render() must not exempt the module, got {diags:?}"
        );
    }

    // --- (g) Solid.js application entry points (Closes #1935) --------------

    // Regression for #1935: a Solid.js entry point mounts the app at module
    // level with `render(() => <App />, root)` imported from `solid-js/web`.
    // That is the entry file's whole purpose and it is never imported, so it
    // must not be flagged.
    #[test]
    fn allows_solid_render_entry_point() {
        let src = "\
            import { render } from 'solid-js/web';\n\
            import App from './App';\n\
            const root = document.getElementById('root');\n\
            render(() => <App />, root!);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "examples/solid/simple/src/index.tsx");
        assert!(
            diags.is_empty(),
            "Solid.js render() entry point is exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_local_render_call_without_solid_import() {
        // A bare top-level `render(...)` with no `solid-js/web` import is an
        // ordinary local call; its module-level side effect still blocks
        // tree-shaking.
        let src = "\
            import { render } from './template';\n\
            render(data);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "local render() without solid-js/web import must still be flagged, got {diags:?}"
        );
    }

    // --- (g) Vue 3 application entry points (Closes #1709) ------------------

    // Regression for #1709: the canonical Vue 3 entry point creates the app with
    // `createApp(App)`, registers plugins/components with `app.use(...)` /
    // `app.component(...)`, and mounts it with `app.mount('#app')`. That is the
    // entry file's whole purpose and it is never imported, so it must not be
    // flagged.
    #[test]
    fn allows_vue_create_app_entry_point() {
        let src = "\
            import { createApp } from 'vue';\n\
            import App from './App.vue';\n\
            import { createPinia } from 'pinia';\n\
            import { router } from './router/resolver';\n\
            const app = createApp(App);\n\
            app.use(createPinia());\n\
            app.use(PiniaColada, {});\n\
            app.use(DataLoaderPlugin, { router: router as any });\n\
            app.component('RouterLink', RouterLink);\n\
            app.component('RouterView', RouterView);\n\
            app.use(router);\n\
            app.mount('#app');\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "packages/playground-file-based/src/main.ts");
        assert!(
            diags.is_empty(),
            "Vue createApp + app.use + app.mount entry point is exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_vue_chained_create_app_entry_point() {
        let src = "\
            import { createApp } from 'vue';\n\
            import App from './App.vue';\n\
            import { router } from './router';\n\
            import { pinia } from './store';\n\
            createApp(App).use(router).use(pinia).mount('#app');\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/main.ts");
        assert!(
            diags.is_empty(),
            "chained createApp().use().mount() entry point is exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_top_level_side_effects_without_vue_mount() {
        // An ordinary imported module with genuine top-level side effects and no
        // Vue app-mount bootstrap chain must still be flagged — neither a local
        // `mount()` helper nor unrelated calls grant the entry-point exemption.
        let src = "\
            import { mount } from './widget';\n\
            mount(document.body);\n\
            registerGlobals();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget.ts");
        assert_eq!(
            diags.len(),
            2,
            "top-level side effects without a createApp().mount() chain must still be flagged, got {diags:?}"
        );
    }

    // --- (g) Preact / Angular application entry points (Closes #1714) -------

    // Regression for #1714: the canonical Preact entry point mounts the app at
    // module level with `render(<App />, root)` imported from `preact`. That is
    // the entry file's whole purpose and it is never imported, so it must not be
    // flagged.
    #[test]
    fn allows_preact_render_entry_point() {
        let src = "\
            import { render } from 'preact';\n\
            import App from './App';\n\
            render(<App />, document.getElementById('app')!);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "examples/preact/basic/src/main.tsx");
        assert!(
            diags.is_empty(),
            "Preact render() entry point is exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_local_render_call_without_preact_import() {
        // A bare top-level `render(...)` with no `preact` import is an ordinary
        // local call; its module-level side effect still blocks tree-shaking.
        let src = "\
            import { render } from './template';\n\
            render(somethingElse);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "local render() without preact import must still be flagged, got {diags:?}"
        );
    }

    // Regression for #1714: the canonical Angular standalone entry point
    // bootstraps the app at module level with
    // `bootstrapApplication(AppComponent, appConfig)` imported from
    // `@angular/platform-browser`. That is the entry file's whole purpose and it
    // is never imported, so it must not be flagged.
    #[test]
    fn allows_angular_bootstrap_application_entry_point() {
        let src = "\
            import { bootstrapApplication } from '@angular/platform-browser';\n\
            import { AppComponent } from './app/app.component';\n\
            import { appConfig } from './app/app.config';\n\
            bootstrapApplication(AppComponent, appConfig);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/main.ts");
        assert!(
            diags.is_empty(),
            "Angular bootstrapApplication() entry point is exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_local_bootstrap_application_call_without_angular_import() {
        // A bare top-level `bootstrapApplication(...)` with no
        // `@angular/platform-browser` import is an ordinary local call; its
        // module-level side effect still blocks tree-shaking.
        let src = "\
            import { bootstrapApplication } from './bootstrap';\n\
            bootstrapApplication(config);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "local bootstrapApplication() without @angular/platform-browser import must still be flagged, got {diags:?}"
        );
    }

    // Regression for #2184: the canonical Angular standalone entry chains
    // `.catch(...)` onto the bootstrap promise to surface bootstrap failures —
    // `bootstrapApplication(App, appConfig).catch(err => console.error(err))`.
    // The top-level expression is a `.catch()` member call whose object is the
    // `bootstrapApplication` call, so the entry detection must unwrap the
    // continuation just like the `listen().catch()` server-entry shape.
    #[test]
    fn allows_angular_bootstrap_application_catch_chained_entry_point() {
        let src = "\
            import { bootstrapApplication } from '@angular/platform-browser';\n\
            import { appConfig } from './app/app.config';\n\
            import { App } from './app/app';\n\
            bootstrapApplication(App, appConfig).catch((err) => console.error(err));\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "examples/angular/filters/src/main.ts");
        assert!(
            diags.is_empty(),
            "bootstrapApplication().catch() is an Angular entry point startup pattern, got {diags:?}"
        );
    }

    // Regression for #2184: the canonical React 18 entry mounts the app with
    // `ReactDOM.createRoot(rootElement).render(<App />)` where `ReactDOM` is the
    // default import from `react-dom/client`. This is the issue's exact example
    // and must not be flagged.
    #[test]
    fn allows_react_dom_default_import_create_root_entry_point() {
        let src = "\
            import ReactDOM from 'react-dom/client';\n\
            import App from './App';\n\
            const rootElement = document.getElementById('root');\n\
            ReactDOM.createRoot(rootElement).render(<App />);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "examples/react/column-dnd/src/main.tsx");
        assert!(
            diags.is_empty(),
            "ReactDOM.createRoot().render() entry point is exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_bootstrap_application_catch_chain_without_angular_import() {
        // Negative space: the `.catch()` continuation only grants the exemption
        // when `bootstrapApplication` is imported from `@angular/platform-browser`.
        // A local `bootstrapApplication(...).catch(...)` is an ordinary top-level
        // side effect and is still flagged.
        let src = "\
            import { bootstrapApplication } from './bootstrap';\n\
            bootstrapApplication(config).catch((err) => console.error(err));\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "local bootstrapApplication().catch() without @angular/platform-browser import must still flag, got {diags:?}"
        );
    }

    // Regression-lock for #1709: the Vue createApp().mount() exemption added by
    // an earlier PR must keep suppressing the canonical Vue entry point.
    #[test]
    fn allows_vue_chained_create_app_entry_point_regression_lock() {
        let src = "\
            import { createApp } from 'vue';\n\
            import App from './App.vue';\n\
            import './index.css';\n\
            createApp(App).mount('#app');\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "examples/vue/filters/src/main.ts");
        assert!(
            diags.is_empty(),
            "Vue createApp().mount() entry point must remain exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_unrelated_top_level_side_effect_call() {
        // A genuine top-level side-effect call unrelated to any framework
        // bootstrap must still be flagged, regardless of file name.
        let diags = crate::rules::test_helpers::run_rule(&Check, "registerGlobals();", "src/main.ts");
        assert_eq!(
            diags.len(),
            1,
            "an unrelated top-level side-effect call must still be flagged, got {diags:?}"
        );
    }

    // --- (h) CLI entry points (Closes #2050) ------------------------------

    // Regression for #2050: a yargs-based CLI entry named `bin.ts` runs the CLI
    // at module level (`yargs(hideBin(process.argv)).parse()`) with no shebang.
    // It is executed directly (`tsx ./bin.ts`), never imported, so the top-level
    // call is intentional and not tree-shakeable.
    #[test]
    fn allows_yargs_bin_ts_cli_entry_without_shebang() {
        let src = "\
            import yargs from 'yargs/yargs';\n\
            import { hideBin } from 'yargs/helpers';\n\
            yargs(hideBin(process.argv))\n\
              .scriptName('rw-server')\n\
              .strict()\n\
              .command('$0', 'start', () => {}, () => {})\n\
              .parse();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "packages/api-server/src/bin.ts");
        assert!(
            diags.is_empty(),
            "bin.ts CLI entry without shebang should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_bin_mts_cli_entry() {
        let src = "\
            process.stdin.pipe(formatter()).pipe(process.stdout);\n\
            process.on('SIGINT', () => {});\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/logFormatter/bin.mts");
        assert!(
            diags.is_empty(),
            "bin.mts CLI entry should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_shebang_cli_entry() {
        // A non-`bin` filename with a `#!` shebang is still a directly-executed
        // script, never imported.
        let src = "\
            #!/usr/bin/env tsx\n\
            import yargs from 'yargs/yargs';\n\
            yargs(process.argv).parse();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/bins/rw-fwtools-attw.ts");
        assert!(
            diags.is_empty(),
            "shebang CLI script should be exempt, got {diags:?}"
        );
    }

    // Regression-lock for #1695: a Redwood binary entry under `src/bins/` opens
    // with a `#!/usr/bin/env node` shebang and bootstraps the real CLI via a
    // top-level computed-member call (`requireFromRwJobs(bins['rw-jobs-worker'])`).
    // The leading shebang marks it as a directly-executed script, never imported,
    // so the top-level call is intentional and not tree-shakeable.
    #[test]
    fn allows_node_shebang_bin_with_computed_member_call() {
        let src = "\
            #!/usr/bin/env node\n\
            import { createRequire } from 'node:module';\n\
            const require = createRequire(import.meta.url);\n\
            const requireFromRwJobs = createRequire(\n\
              require.resolve('@redwoodjs/jobs/package.json'),\n\
            );\n\
            const bins = requireFromRwJobs('./package.json')['bin'];\n\
            requireFromRwJobs(bins['rw-jobs-worker']);\n";
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            src,
            "packages/core/src/bins/rw-jobs-worker.ts",
        );
        assert!(
            diags.is_empty(),
            "node shebang bin entry should be exempt, got {diags:?}"
        );
    }

    // Negative-space guard for #1695: the same top-level call shape in an
    // ordinary, non-shebang `src/` module is still a tree-shaking hazard and
    // must be flagged. The shebang is the only thing that grants the exemption.
    #[test]
    fn still_flags_non_shebang_module_with_computed_member_call() {
        let src = "\
            import { createRequire } from 'node:module';\n\
            const require = createRequire(import.meta.url);\n\
            const requireFromRwJobs = createRequire(\n\
              require.resolve('@redwoodjs/jobs/package.json'),\n\
            );\n\
            const bins = requireFromRwJobs('./package.json')['bin'];\n\
            requireFromRwJobs(bins['rw-jobs-worker']);\n";
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            src,
            "packages/core/src/loadBins.ts",
        );
        assert_eq!(
            diags.len(),
            1,
            "ordinary non-shebang module must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_non_bin_module_with_side_effect() {
        // `binary.ts` merely contains "bin" — not the `bin` convention — and has
        // no shebang, so its top-level side effect still blocks tree-shaking.
        let src = "\
            import yargs from 'yargs/yargs';\n\
            yargs(process.argv).parse();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/binary.ts");
        assert_eq!(
            diags.len(),
            1,
            "binary.ts is an ordinary module and must still be flagged, got {diags:?}"
        );
    }

    // --- (h2) Node.js CLI script entry points — main() pattern (Closes #1629)

    // Regression for #1629: a Node.js CLI script defines `async function main()`
    // and invokes it at the top level (`main();`). The file is run directly
    // (`tsx ./index.mts`), never imported as a library, so the entry invocation
    // is the program's purpose, not a tree-shakeable side effect.
    #[test]
    fn allows_main_entry_invocation() {
        let src = "\
            import { extractApi } from './extract';\n\
            async function main() {\n\
              await extractApi(process.argv.slice(2));\n\
            }\n\
            main();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "tools/adev-api-extraction/index.mts");
        assert!(
            diags.is_empty(),
            "a self-invoked local main() CLI entry should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_main_catch_chained_entry_invocation() {
        // The idiomatic async-CLI form chains `.catch(...)` onto `main()` to
        // surface failures; unwrapping the continuation recovers the `main` call.
        let src = "\
            const main = async () => { await run(); };\n\
            main().catch((err) => { console.error(err); process.exit(1); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/cli.ts");
        assert!(
            diags.is_empty(),
            "main().catch() CLI entry should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_main_call_without_local_definition() {
        // `main` is imported, not defined in this module — a top-level `main()`
        // here is an ordinary side-effect call, not the self-invocation entry
        // pattern, so it stays flagged.
        let src = "\
            import { main } from './app';\n\
            main();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "imported main() is not a CLI entry self-invocation and must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_local_function_other_than_main() {
        // A locally-defined `boot()` invoked at the top level is NOT the `main`
        // convention — a genuine top-level side effect that still blocks
        // tree-shaking. The exemption is keyed to the `main` name, not any
        // self-invoked local function.
        let src = "\
            function boot() { registerGlobals(); }\n\
            boot();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "a self-invoked local boot() is not the main() entry pattern and must still be flagged, got {diags:?}"
        );
    }

    // --- (g2) Benchmark / profiling harness scripts (Closes #1913) --------

    // Regression for #1913: a `profile-*.ts` harness lives in `src/` but is run
    // directly (`bun src/native/profile-pipeline.ts`). Its entire body is
    // top-level `console.log`/`bench(...)` calls — the harness's payload — and
    // it is never imported as a module, so the calls are intentional.
    #[test]
    fn allows_profile_prefixed_harness_script() {
        let src = "\
            import './bunProfileGlobals';\n\
            import { bench } from './profileHarness';\n\
            console.log('\\n=== transformDecl single-pair ===');\n\
            bench('passthrough', 200_000, () => transformDecl('transform', 'scale(2)'));\n\
            bench('numeric', 200_000, () => transformDecl('padding-top', '8px'));\n";
        let diags =
            crate::rules::test_helpers::run_rule(&Check, src, "packages/styled-components/src/native/profile-pipeline.ts");
        assert!(
            diags.is_empty(),
            "profile-*.ts harness script should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_bench_prefixed_harness_script() {
        let src = "\
            import { bench } from './harness';\n\
            bench('insert', 1000, () => insert());\n";
        let diags =
            crate::rules::test_helpers::run_rule(&Check, src, "src/bench-insert.ts");
        assert!(
            diags.is_empty(),
            "bench-*.ts harness script should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_harness_under_benchmarks_directory() {
        let src = "\
            import { run } from './runner';\n\
            run();\n";
        let diags =
            crate::rules::test_helpers::run_rule(&Check, src, "benchmarks/parse.ts");
        assert!(
            diags.is_empty(),
            "script under benchmarks/ should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_library_module_with_profile_substring_in_name() {
        // `profileService.ts` is an ordinary library module — it merely contains
        // "profile", it is not a `profile-*` harness — so its accidental
        // top-level side effect still blocks tree-shaking.
        let diags =
            crate::rules::test_helpers::run_rule(&Check, "loadProfiles();", "src/profileService.ts");
        assert_eq!(
            diags.len(),
            1,
            "profileService.ts is a library module and must still be flagged, got {diags:?}"
        );
    }

    // --- (h) Gulp task-registration files (Closes #2024) ------------------

    // Regression for #2024: a Gulp build file imports `gulp` and registers
    // tasks at the top level via `task(...)`. That IS the gulpfile's purpose —
    // Gulp runs the registrations by importing the file — so the top-level
    // calls are intentional side effects, not tree-shakeable library code.
    #[test]
    fn allows_gulp_task_registration_file() {
        let src = "\
            import { task } from 'gulp';\n\
            async function run(script) { return script; }\n\
            task('install:samples', async () => run('npm install'));\n\
            task('build:samples', async () => run('npm run build'));\n";
        let diags =
            crate::rules::test_helpers::run_rule(&Check, src, "tools/gulp/tasks/samples.ts");
        assert!(
            diags.is_empty(),
            "gulp task-registration file should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_gulp_namespace_series_parallel() {
        // `gulp.task(...)` member calls plus `series(...)`/`parallel(...)`
        // registrations from a `gulp` namespace import are all exempt.
        let src = "\
            import gulp, { series, parallel } from 'gulp';\n\
            gulp.task('clean', () => {});\n\
            series('a', 'b');\n\
            parallel('c', 'd');\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "gulpfile.ts");
        assert!(
            diags.is_empty(),
            "gulp.task/series/parallel registrations should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_gulp_require_task_registration() {
        let src = "\
            const gulp = require('gulp');\n\
            gulp.task('default', () => {});\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "gulpfile.js");
        assert!(
            diags.is_empty(),
            "require('gulp') task registration should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_local_task_call_without_gulp_import() {
        // No `gulp` import — a local `task()` call is an ordinary top-level side
        // effect that still blocks tree-shaking.
        let src = "\
            import { task } from './scheduler';\n\
            task('do-it', () => {});\n\
            doSideEffect();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/jobs.ts");
        assert_eq!(
            diags.len(),
            2,
            "non-gulp module with top-level side effects must still be flagged, got {diags:?}"
        );
    }

    // --- (i) Storybook addon manager entry files (Closes #2058) -----------

    // Regression for #2058: a Storybook addon `manager.tsx` imports `addons`
    // from a Storybook manager package and registers the addon at the top level
    // via `addons.register(...)` / `addons.add(...)`. The Storybook manager
    // bundle loads these entry files to run exactly those registrations, so the
    // top-level calls are intentional side effects, not tree-shakeable code.
    #[test]
    fn allows_storybook_addon_manager_entry() {
        let src = "\
            import { addons, types } from 'storybook/manager-api';\n\
            addons.register(ADDON_ID, (api) => {\n\
              addons.add(PANEL_ID, {\n\
                title: Title,\n\
                type: types.PANEL,\n\
                render: () => null,\n\
              });\n\
            });\n";
        let diags =
            crate::rules::test_helpers::run_rule(&Check, src, "code/addons/a11y/src/manager.tsx");
        assert!(
            diags.is_empty(),
            "Storybook addon manager entry should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_storybook_set_config_from_addons_package() {
        // Legacy `@storybook/addons` default import + `setConfig` registration.
        let src = "\
            import addons from '@storybook/addons';\n\
            addons.setConfig({ panelPosition: 'right' });\n";
        let diags =
            crate::rules::test_helpers::run_rule(&Check, src, "code/core/src/manager/setup.ts");
        assert!(
            diags.is_empty(),
            "addons.setConfig from @storybook/addons should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_addons_register_without_storybook_import() {
        // No Storybook import — a local `addons.register()` is an ordinary
        // top-level side effect that still blocks tree-shaking.
        let src = "\
            import { addons } from './my-registry';\n\
            addons.register('x', () => {});\n\
            doSideEffect();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widgets.ts");
        assert_eq!(
            diags.len(),
            2,
            "non-Storybook module with top-level side effects must still be flagged, got {diags:?}"
        );
    }

    // --- MCP server request-handler registration (Closes #1634) -----------

    // Regression for #1634: an MCP server module imports `Server` from
    // `@modelcontextprotocol/sdk`, constructs and exports the instance, and
    // registers request handlers at module scope via
    // `server.setRequestHandler(Schema, handler)`. The SDK requires this at
    // module init on the exported server, so the registrations are intentional
    // initialization, not tree-shakeable library code.
    #[test]
    fn allows_mcp_set_request_handler_registration() {
        let src = "\
            import { Server } from '@modelcontextprotocol/sdk/server/index.js';\n\
            import { ListToolsRequestSchema, CallToolRequestSchema } from '@modelcontextprotocol/sdk/types.js';\n\
            export const server = new Server(\n\
              { name: 'shadcn', version: '1.0.0' },\n\
              { capabilities: { resources: {}, tools: {} } },\n\
            );\n\
            server.setRequestHandler(ListToolsRequestSchema, async () => {\n\
              return { tools: [] };\n\
            });\n\
            server.setRequestHandler(CallToolRequestSchema, async (request) => {\n\
              return { content: [] };\n\
            });\n";
        let diags =
            crate::rules::test_helpers::run_rule(&Check, src, "packages/shadcn/src/mcp/index.ts");
        assert!(
            diags.is_empty(),
            "MCP server handler registration should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_set_request_handler_without_mcp_import() {
        // No `@modelcontextprotocol/sdk` import — a stray `.setRequestHandler`
        // on an unrelated object is an ordinary top-level side effect.
        let src = "\
            import { emitter } from './bus';\n\
            emitter.setRequestHandler('x', () => {});\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "non-MCP module with a top-level side effect must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_unrelated_side_effect_in_mcp_file() {
        // The MCP exemption gates on the `setRequestHandler` shape, but an MCP
        // module with only an unrelated top-level side effect (no handler
        // registration) is not an MCP server entry and stays flagged.
        let src = "\
            import { Server } from '@modelcontextprotocol/sdk/server/index.js';\n\
            doSideEffect();\n";
        let diags =
            crate::rules::test_helpers::run_rule(&Check, src, "packages/shadcn/src/mcp/index.ts");
        assert_eq!(
            diags.len(),
            1,
            "unrelated top-level side effect must still be flagged, got {diags:?}"
        );
    }

    // --- (j) Data-initialization `forEach` (Closes #2033) ------------------

    // Regression for #2033: a module-level `forEach` populating a locally
    // declared `Map` from a locally declared `const` array is a pure data
    // build — it reads no external state and mutates only module-local state.
    #[test]
    fn allows_foreach_populating_local_map() {
        let src = "\
            const svg_attributes = 'accent-height accumulate'.split(' ');\n\
            const svg_attribute_lookup = new Map();\n\
            svg_attributes.forEach((name) => {\n\
              svg_attribute_lookup.set(name.toLowerCase(), name);\n\
            });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/fix-attribute-casing.js");
        assert!(
            diags.is_empty(),
            "forEach populating a local Map is data init, got {diags:?}"
        );
    }

    #[test]
    fn allows_foreach_set_add_concise_body() {
        // Concise arrow body, `Set.add`.
        let src = "\
            const items = getItems();\n\
            const seen = new Set();\n\
            items.forEach((it) => seen.add(it.id));\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/lookup.ts");
        assert!(
            diags.is_empty(),
            "forEach populating a local Set is data init, got {diags:?}"
        );
    }

    #[test]
    fn allows_foreach_object_index_assignment() {
        // `obj[k] = v` / `obj.k = v` into a local object.
        let src = "\
            const entries = [['a', 1], ['b', 2]];\n\
            const lookup = {};\n\
            entries.forEach(([k, v]) => { lookup[k] = v; });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/lookup.ts");
        assert!(
            diags.is_empty(),
            "forEach assigning into a local object is data init, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_foreach_calling_external_function() {
        // The callback invokes a free function — a genuine side effect.
        let src = "\
            const items = getItems();\n\
            items.forEach((it) => { registerSideEffect(it); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/effect.ts");
        assert_eq!(
            diags.len(),
            1,
            "forEach calling a free function must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_foreach_mutating_non_local_receiver() {
        // The lookup is not declared in this module (imported/global) — the
        // mutation escapes the module, so it stays a flagged side effect.
        let src = "\
            import { registry } from './registry';\n\
            const items = getItems();\n\
            items.forEach((it) => { registry.set(it.k, it.v); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/effect.ts");
        assert_eq!(
            diags.len(),
            1,
            "forEach mutating an imported receiver must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_foreach_with_impure_value_argument() {
        // The value passed to `.set` is produced by a free function call — an
        // embedded side effect, so the exemption must not apply.
        let src = "\
            const items = getItems();\n\
            const lookup = new Map();\n\
            items.forEach((it) => { lookup.set(it.k, sideEffectfulCompute(it)); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/effect.ts");
        assert_eq!(
            diags.len(),
            1,
            "forEach whose value comes from a free call must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_foreach_on_non_local_receiver_array() {
        // The receiver array is imported, not a module-local const — the
        // iteration source escapes the module, so it stays flagged.
        let src = "\
            import { data } from './data';\n\
            const lookup = new Map();\n\
            data.forEach((d) => { lookup.set(d.k, d.v); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/effect.ts");
        assert_eq!(
            diags.len(),
            1,
            "forEach over an imported array must still be flagged, got {diags:?}"
        );
    }

    // --- (k) Builder/fluent config on a same-scope const built with `new`
    //         (Closes #1964) ------------------------------------------------

    // Regression for #1964: configuring a freshly-constructed module-local
    // object (`const obj = new ...(); obj.method(...)`) mutates only that local
    // object — no external side effect, no tree-shaking hazard.
    #[test]
    fn allows_config_call_on_const_built_with_new() {
        let src = "\
            const invisibleLayer = new THREE.Layers();\n\
            invisibleLayer.set(4);\n\
            const group = new THREE.Group();\n\
            group.add(mesh);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/demos/Layers.tsx");
        assert!(
            diags.is_empty(),
            "config calls on a same-scope const built with new are exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_multiple_fluent_config_calls_on_const_built_with_new() {
        let src = "\
            const dracoLoader = new DRACOLoader();\n\
            dracoLoader.setDecoderPath('https://x');\n\
            dracoLoader.preload();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/demos/Activity.tsx");
        assert!(
            diags.is_empty(),
            "multiple fluent config calls on a const built with new are exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_method_call_on_imported_singleton() {
        // `registry` is imported, not a same-scope const-with-new — the mutation
        // escapes the module, so it stays a flagged side effect.
        let src = "\
            import { registry } from './registry';\n\
            registry.register(plugin);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "method call on an imported singleton must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_config_call_on_const_built_with_plain_call() {
        // The receiver's init is a plain call, not `new` — the object may be a
        // shared/external instance, so the mutation stays flagged.
        let src = "\
            const x = getThing();\n\
            x.mutate();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "config call on a const whose init is a plain call must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_bare_top_level_side_effect_call() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "initGlobalState();", "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "a bare top-level side-effect call must still be flagged, got {diags:?}"
        );
    }

    // --- (l) Exports augmentation via `Object.assign` (Closes #1906) -------

    // Regression for #1906: a library entry attaches its secondary namespace to
    // the default export with `Object.assign(styled, secondary)` before
    // re-exporting it. That augments the module's own export object — the
    // standard library pattern (React does `React.useState = useState`) — so it
    // is module initialization, not an external side effect.
    #[test]
    fn allows_object_assign_onto_default_export() {
        let src = "\
            import * as secondary from './base';\n\
            import styled from './constructors/styled';\n\
            Object.assign(styled, secondary);\n\
            export default styled;\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/index-standalone.ts");
        assert!(
            diags.is_empty(),
            "Object.assign onto a re-exported binding is exports augmentation, got {diags:?}"
        );
    }

    #[test]
    fn allows_object_assign_onto_named_export() {
        let src = "\
            import * as extra from './extra';\n\
            const api = makeApi();\n\
            Object.assign(api, extra);\n\
            export { api };\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/api.ts");
        assert!(
            diags.is_empty(),
            "Object.assign onto a named-exported binding is exports augmentation, got {diags:?}"
        );
    }

    #[test]
    fn allows_object_assign_onto_commonjs_exports() {
        let src = "Object.assign(exports, require('./extra'));\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/index.js");
        assert!(
            diags.is_empty(),
            "Object.assign onto the CommonJS exports object is exports augmentation, got {diags:?}"
        );
    }

    #[test]
    fn allows_object_assign_onto_module_exports() {
        let src = "Object.assign(module.exports, require('./extra'));\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/index.js");
        assert!(
            diags.is_empty(),
            "Object.assign onto module.exports is exports augmentation, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_object_assign_onto_imported_singleton() {
        // The target is imported, not re-exported by this module — augmenting it
        // is an external side effect, so it stays flagged.
        let src = "\
            import { registry } from './registry';\n\
            import * as extra from './extra';\n\
            Object.assign(registry, extra);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget.ts");
        assert_eq!(
            diags.len(),
            1,
            "Object.assign onto an imported singleton must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_object_assign_onto_global() {
        // Mutating a global (`window`) is a genuine side effect — `window` is
        // not a re-exported binding, so the call stays flagged.
        let src = "Object.assign(window, { __APP__: true });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/boot.ts");
        assert_eq!(
            diags.len(),
            1,
            "Object.assign onto a global must still be flagged, got {diags:?}"
        );
    }

    // --- (n) library entry barrels (Closes #2084) --------------------------

    // Regression for #2084: zod's `packages/zod/src/v4/classic/external.ts` is
    // the package's public entry barrel — it re-exports the full API with
    // `export *` and registers the default English locale at module load with a
    // top-level `config(en())` call. The barrel is imported to obtain the API,
    // never as a tree-shaking target, so the registration call is intentional
    // initialization, not a tree-shaking hazard.
    #[test]
    fn allows_config_call_in_entry_barrel() {
        let src = "\
            export * from './schemas.js';\n\
            export * from './checks.js';\n\
            import { config } from '../core/index.js';\n\
            import en from '../locales/en.js';\n\
            config(en());\n";
        let diags =
            crate::rules::test_helpers::run_rule(&Check, src, "packages/zod/src/v4/classic/external.ts");
        assert!(
            diags.is_empty(),
            "a top-level registration call in an export-* entry barrel should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_export_star_namespace_entry_barrel() {
        // `export * as ns from "..."` is also a star re-export.
        let src = "\
            export * as schemas from './schemas.js';\n\
            export * as checks from './checks.js';\n\
            registerDefaults();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/index.ts");
        assert!(
            diags.is_empty(),
            "a registration call in a namespaced export-* barrel should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_side_effect_in_non_barrel_module() {
        // Real logic, not just re-exports: a single `export *` does not dominate
        // the four top-level side-effecting calls, so the module is not a barrel
        // and stays flagged.
        let src = "\
            export * from './types.js';\n\
            analytics.track('load');\n\
            db.connect();\n\
            cache.warm();\n\
            logger.info('ready');\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/service.ts");
        assert_eq!(
            diags.len(),
            4,
            "a module not dominated by export-* re-exports must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_top_level_call_without_export_star() {
        // No `export *` re-export at all — not a barrel, stays flagged even though
        // the file also has a named export.
        let src = "\
            export { thing } from './thing.js';\n\
            config(en());\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/external.ts");
        assert_eq!(
            diags.len(),
            1,
            "a top-level call in a file with no export-* re-export must still be flagged, got {diags:?}"
        );
    }

    // --- (g) examples/ demonstration scripts (Closes #1918) ----------------

    // Regression for #1918: date-fns ships small runnable demonstration scripts
    // under `examples/` whose whole purpose is a top-level `console.log(...)`
    // showing library output. They are never imported as library modules, so the
    // tree-shaking concern does not apply. The relaxed-dir gate exempts them.
    #[test]
    fn allows_example_dir_demo_script() {
        let src = "\
            import { maxTime } from 'date-fns/constants';\n\
            console.log(maxTime === 8640000000000000);\n";
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "pkgs/core/examples/node-esm/constants.js",
        );
        assert!(
            diags.is_empty(),
            "demonstration scripts under examples/ are exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_demo_dir_demo_script() {
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "console.log(formatDate(new Date()));\n",
            "demo/usage.ts",
        );
        assert!(
            diags.is_empty(),
            "demonstration scripts under demo/ are exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_library_module_in_src() {
        // A genuine library module with an accidental top-level side effect is
        // still flagged — the relaxed-dir gate only covers demonstration dirs.
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "console.log('loaded');\n",
            "src/index.ts",
        );
        assert_eq!(
            diags.len(),
            1,
            "library module under src/ must still be flagged, got {diags:?}"
        );
    }

    // --- (h) codemod .actual/.expected snapshot fixtures (Closes #1353) ----

    // Regression for #1353: material-ui's codemod packages keep jscodeshift
    // input/output snapshots as `*.actual.js` / `*.expected.js` files whose
    // top-level calls (`fn({...})`) are intentional test data. They are read as
    // text by the codemod harness, never imported or bundled, so the
    // tree-shaking concern does not apply. The codemod-snapshot infix marks them
    // as test files, which the `skip_in_test_dir` gate then exempts.
    #[test]
    fn allows_codemod_actual_fixture() {
        let src = "\
            fn({\n\
              MuiTypography: { defaultProps: {} },\n\
            });\n\
            fn({\n\
              MuiTypography: { defaultProps: { className: \"my-class\" } },\n\
            });\n";
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "src/deprecations/typography-props/test-cases/theme.actual.js",
        );
        assert!(
            diags.is_empty(),
            "codemod .actual.js snapshot fixtures are exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_codemod_expected_fixture() {
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "fn({ MuiTypography: {} });\n",
            "src/v1.0.0/color-imports/theme.expected.ts",
        );
        assert!(
            diags.is_empty(),
            "codemod .expected.ts snapshot fixtures are exempt, got {diags:?}"
        );
    }

    // --- code-generation utility scripts (#1813) --------------------------

    // Regression for #1813: a `generate.ts` script under `src/utils/` iterates a
    // single processing function over a dataset and writes output files. Every
    // top-level call is the same callee; the file imports `fs`. Such scripts are
    // run directly (`tsx generate.ts`), never imported, so the repeated top-level
    // calls are intentional, not a tree-shaking hazard.
    #[test]
    fn allows_data_generation_script_with_uniform_calls() {
        let src = "\
            import * as path from 'node:path';\n\
            import * as fs from 'fs';\n\
            import { arSA } from '../ar-SA';\n\
            import { beBY } from '../be-BY';\n\
            import { bgBG } from '../bg-BG';\n\
            function run(locale, localeId) { fs.writeFileSync(localeId, ''); }\n\
            run(arSA, 'ar-SA');\n\
            run(beBY, 'be-BY');\n\
            run(bgBG, 'bg-BG');\n";
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            src,
            "packages/localizations/src/utils/generate.ts",
        );
        assert!(
            diags.is_empty(),
            "data-generation script with uniform top-level calls should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_uniform_calls_without_fs_import() {
        // Without a filesystem import the file is not a code-generation script:
        // repeated same-callee top-level calls are still flagged.
        let src = "\
            run(1);\n\
            run(2);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/utils/generate.ts");
        assert_eq!(
            diags.len(),
            2,
            "uniform top-level calls without an fs import must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_single_top_level_call_with_fs_import() {
        // A single top-level call is the canonical thing to flag — the
        // data-generation shape requires the uniform-iteration pattern (>=2 calls).
        let src = "\
            import * as fs from 'fs';\n\
            run(1);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/utils/generate.ts");
        assert_eq!(
            diags.len(),
            1,
            "a single top-level call must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_heterogeneous_calls_with_fs_import() {
        // Heterogeneous top-level callees are not the uniform-iteration shape of a
        // generation script; they remain flagged even with an fs import.
        let src = "\
            import * as fs from 'fs';\n\
            doA();\n\
            doB();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/utils/generate.ts");
        assert_eq!(
            diags.len(),
            2,
            "heterogeneous top-level calls must still be flagged, got {diags:?}"
        );
    }

    // --- (m) Vue 2 mixin-builder & prototype-patcher (Closes #1748) --------

    // Regression for #1748: Vue 2 assembles its constructor at module scope by
    // passing the local `Vue` function through a chain of mixin builders that
    // attach methods to its prototype, then exports it. The whole purpose of
    // importing the module is to obtain the fully assembled export, so the
    // builder calls are intentional initialization, not tree-shakeable code.
    #[test]
    fn allows_vue_mixin_builder_calls() {
        let src = "\
            import { initMixin } from './init';\n\
            import { stateMixin } from './state';\n\
            import { renderMixin } from './render';\n\
            import { eventsMixin } from './events';\n\
            import { lifecycleMixin } from './lifecycle';\n\
            import type { GlobalAPI } from 'types/global-api';\n\
            function Vue(options) {\n\
              this._init(options);\n\
            }\n\
            initMixin(Vue);\n\
            stateMixin(Vue);\n\
            eventsMixin(Vue);\n\
            lifecycleMixin(Vue);\n\
            renderMixin(Vue);\n\
            export default Vue as unknown as GlobalAPI;\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/core/instance/index.ts");
        assert!(
            diags.is_empty(),
            "Vue 2 mixin-builder calls on an exported binding should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_mixin_builder_call_on_named_exported_const() {
        // The exported binding may be a named export rather than the default.
        let src = "\
            import { initMixin } from './init';\n\
            const Widget = function (opts) { this.opts = opts; };\n\
            initMixin(Widget);\n\
            export { Widget };\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/widget/index.ts");
        assert!(
            diags.is_empty(),
            "mixin builder on a named-exported binding should be exempt, got {diags:?}"
        );
    }

    // Regression for #1748: Vue 2 intercepts the mutating Array prototype methods
    // by iterating `methodsToPatch` (a local const) and calling `def(arrayMethods,
    // …)` — `arrayMethods` is exported. Importing the module exists to obtain the
    // patched export, so the iteration is intended initialization.
    #[test]
    fn allows_prototype_patcher_foreach_on_exported_object() {
        let src = "\
            import { def } from '../util/index';\n\
            const arrayProto = Array.prototype;\n\
            export const arrayMethods = Object.create(arrayProto);\n\
            const methodsToPatch = ['push', 'pop', 'shift', 'unshift', 'splice', 'sort', 'reverse'];\n\
            methodsToPatch.forEach(function (method) {\n\
              const original = arrayProto[method];\n\
              def(arrayMethods, method, function mutator(...args) {\n\
                return original.apply(this, args);\n\
              });\n\
            });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/core/observer/array.ts");
        assert!(
            diags.is_empty(),
            "prototype-patcher forEach on an exported object should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_prototype_patcher_foreach_with_member_assignment() {
        // The patch can also be a member assignment onto the exported object.
        let src = "\
            export const handlers = {};\n\
            const events = ['click', 'hover'];\n\
            events.forEach((evt) => { handlers[evt] = makeHandler; });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/dom/handlers.ts");
        assert!(
            diags.is_empty(),
            "forEach assigning onto an exported object should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_builder_call_on_non_exported_binding() {
        // `init(plugin)` whose argument is an imported (non-exported) binding is
        // an ordinary top-level side effect that still blocks tree-shaking.
        let src = "\
            import { plugin } from './plugin';\n\
            register(plugin);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/bootstrap.ts");
        assert_eq!(
            diags.len(),
            1,
            "builder call whose argument is not an exported binding must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_bare_builder_call_without_argument() {
        // A bare top-level call with no arguments cannot be a mixin builder.
        let diags = crate::rules::test_helpers::run_rule(&Check, "initGlobals();", "src/index.ts");
        assert_eq!(
            diags.len(),
            1,
            "an argument-less top-level call must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_foreach_patching_non_exported_object() {
        // The patched object is module-local but NOT exported, and the body calls
        // a free function — a genuine side effect that escapes nothing observable
        // to consumers, so it stays flagged.
        let src = "\
            import { def } from './util';\n\
            const internal = {};\n\
            const keys = ['a', 'b'];\n\
            keys.forEach((k) => { def(internal, k, value); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/internal.ts");
        assert_eq!(
            diags.len(),
            1,
            "forEach patching a non-exported object must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_foreach_patching_imported_object() {
        // The patched object is imported (escapes the module) — the mutation is a
        // genuine external side effect even though the receiver array is local.
        let src = "\
            import { def } from './util';\n\
            import { registry } from './registry';\n\
            const keys = ['a', 'b'];\n\
            keys.forEach((k) => { def(registry, k, value); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/patch.ts");
        assert_eq!(
            diags.len(),
            1,
            "forEach patching an imported object must still be flagged, got {diags:?}"
        );
    }

    // Regression for #1694: a package-root build script (`build.ts`) the
    // package's `scripts.build` runs directly via `tsx` is a one-shot
    // executable, never imported as a library, so its top-level build steps are
    // not initialization side effects.
    #[test]
    fn allows_package_root_build_script_invoked_by_scripts() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"@redwoodjs/cli-helpers","scripts":{"build":"tsx ./build.ts"},"main":"./dist/cjs/index.js","exports":{".":{"default":{"default":"./dist/index.js"}}}}"#,
        )
        .unwrap();
        let src = "\
            import { writeFileSync } from 'node:fs';\n\
            import { build, defaultBuildOptions } from '@redwoodjs/framework-tools';\n\
            await build({ buildOptions: { ...defaultBuildOptions, format: 'esm', packages: 'external' } });\n\
            await build({ buildOptions: { ...defaultBuildOptions, outdir: 'dist/cjs', packages: 'external' } });\n\
            writeFileSync('dist/cjs/package.json', JSON.stringify({ type: 'commonjs' }));\n\
            writeFileSync('dist/package.json', JSON.stringify({ type: 'module' }));\n";
        let project = crate::project::ProjectCtx::empty();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            dir.path().join("build.ts"),
            &project,
            file,
        );
        assert!(
            diags.is_empty(),
            "package-root build.ts run by scripts.build must be exempt, got {diags:?}"
        );
    }

    // Negative-space guard for #1694: an ordinary library module the package's
    // `scripts` never invoke — even one sitting next to the same `package.json`
    // — keeps its genuine top-level side effects flagged.
    #[test]
    fn still_flags_ordinary_module_not_invoked_by_scripts() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"@redwoodjs/cli-helpers","scripts":{"build":"tsx ./build.ts"},"main":"./dist/cjs/index.js"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let src = "\
            import { registerGlobals } from '@redwoodjs/framework-tools';\n\
            registerGlobals();\n";
        let project = crate::project::ProjectCtx::empty();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            dir.path().join("src/loader.ts"),
            &project,
            file,
        );
        assert_eq!(
            diags.len(),
            1,
            "an imported src/ module the scripts never run must still be flagged, got {diags:?}"
        );
    }

    /// Build a `ProjectCtx` rooted at a tempdir holding the given files, so
    /// `project_root`-anchored path classifiers resolve against a real root.
    /// Returns `(tempdir, project, canonical paths)` — drop `tempdir` last.
    fn project_rooted_at_tempdir(
        files: &[(&str, &str)],
    ) -> (tempfile::TempDir, crate::project::ProjectCtx, Vec<std::path::PathBuf>) {
        let dir = tempfile::TempDir::new().unwrap();
        let mut source_files = Vec::new();
        let mut paths = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, content).unwrap();
            let lang = crate::files::Language::from_path(&p).unwrap();
            source_files.push(crate::files::SourceFile { path: p.clone(), language: lang });
            paths.push(std::fs::canonicalize(&p).unwrap());
        }
        let refs: Vec<&crate::files::SourceFile> = source_files.iter().collect();
        let project = crate::project::ProjectCtx::load(&refs, &crate::config::Config::default());
        (dir, project, paths)
    }

    // Regression for #1657: an inline CLI/build script under a top-level
    // `scripts/` directory (apollo-server's `scripts/precompile.mjs`) is run
    // directly via `node scripts/precompile.mjs`, never imported as a library,
    // so its top-level inline side effects are the entry point's purpose.
    #[test]
    fn allows_inline_cli_script_in_scripts_dir() {
        let src = "\
            import path from 'path';\n\
            import { readFileSync, writeFileSync, mkdirSync } from 'fs';\n\
            const { version } = JSON.parse(readFileSync(path.join('packages', 'package.json'), 'utf-8'));\n\
            mkdirSync('packages/server/src/generated', { recursive: true });\n\
            writeFileSync('packages/server/src/generated/packageVersion.ts', `export const packageVersion = ${version};\\n`);\n";
        let (_dir, project, paths) = project_rooted_at_tempdir(&[
            ("package.json", r#"{"name":"@apollo/server","scripts":{"precompile":"node scripts/precompile.mjs"}}"#),
            ("scripts/precompile.mjs", src),
        ]);
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            &paths[1],
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        );
        assert!(
            diags.is_empty(),
            "scripts/precompile.mjs is a top-level CLI/automation entry, got {diags:?}"
        );
    }

    // Negative-space guard for #1657: a genuine library module under `src/` —
    // even one carrying `scripts` deeper in its path — keeps its top-level side
    // effects flagged. The exemption is anchored to top-level `scripts/` only.
    #[test]
    fn still_flags_library_module_in_nested_scripts_dir() {
        let src = "\
            import { registerGlobals } from './globals';\n\
            registerGlobals();\n";
        let (_dir, project, paths) = project_rooted_at_tempdir(&[
            ("package.json", r#"{"name":"app"}"#),
            ("src/scripts/loader.ts", src),
        ]);
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            &paths[1],
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        );
        assert_eq!(
            diags.len(),
            1,
            "a nested src/scripts/ library module must still be flagged, got {diags:?}"
        );
    }

    // --- (i) Node.js CLI entry-point setup (Closes #1631) -----------------

    // Regression for #1631: a CLI entry point registers process signal handlers
    // at module scope so the program shuts down gracefully on interruption.
    // `process.on("SIGINT"/"SIGTERM", …)` is the program's intended setup, not a
    // tree-shakeable side effect.
    #[test]
    fn allows_process_signal_handler_registration() {
        let src = "\
            process.on('SIGINT', () => process.exit(0));\n\
            process.on('SIGTERM', () => process.exit(0));\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/server.ts");
        assert!(
            diags.is_empty(),
            "process.on('SIG...') signal-handler registration should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_non_signal_process_on_subscription() {
        // A `process.on(...)` for a non-signal event (`'exit'`, `'message'`, …)
        // is not a graceful-shutdown signal handler; its top-level registration
        // remains a side effect.
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            "process.on('exit', () => flush());",
            "src/index.ts",
        );
        assert_eq!(
            diags.len(),
            1,
            "process.on('exit', …) is not a signal handler and must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_emitter_on_subscription() {
        // A `.on('SIGINT', …)` on any other emitter is an ordinary top-level
        // subscription; only the bare `process` global is the CLI signal-handler
        // shape.
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            "emitter.on('SIGINT', () => shutdown());",
            "src/index.ts",
        );
        assert_eq!(
            diags.len(),
            1,
            "a .on() subscription on a non-process emitter must still be flagged, got {diags:?}"
        );
    }

    // Regression for #1631: commander.js subcommands are registered at module
    // scope by chaining `.command(...).description(...).option(...).action(...)`
    // on a `Command` instance. The chain is the program's intended setup
    // observed only through the command object, not a tree-shakeable side effect.
    #[test]
    fn allows_commander_subcommand_chain() {
        let src = "\
            import { Command } from 'commander';\n\
            export const mcp = new Command().name('mcp');\n\
            mcp\n\
              .command('init')\n\
              .description('Initialize MCP configuration for your client')\n\
              .option('--client <client>', 'MCP client')\n\
              .action(async (opts, command) => { await runInit(opts); });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/commands/mcp.ts");
        assert!(
            diags.is_empty(),
            "commander .command().action() subcommand chain should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_commander_subcommand_chain_rooted_at_program() {
        let src = "\
            import { program } from 'commander';\n\
            program\n\
              .command('build')\n\
              .action(() => build());\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/cli.ts");
        assert!(
            diags.is_empty(),
            "commander chain rooted at a Command instance should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_fluent_chain_without_action() {
        // A fluent chain with `.command(...)` but no `.action(...)` is not the
        // commander subcommand-registration shape; requiring both keeps the
        // exemption precise.
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            "registry.command('install').describe('x');",
            "src/index.ts",
        );
        assert_eq!(
            diags.len(),
            1,
            "a fluent chain missing .action() must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_fluent_chain_without_command() {
        // A fluent chain with `.action(...)` but no `.command(...)` is not the
        // commander subcommand-registration shape.
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            "queue.enqueue(job).action(() => run());",
            "src/index.ts",
        );
        assert_eq!(
            diags.len(),
            1,
            "a fluent chain missing .command() must still be flagged, got {diags:?}"
        );
    }

    // Negative-space guard for #1631: a genuine top-level side effect (a
    // mutating call on an imported singleton) in the same kind of CLI file is
    // not a signal handler or a commander chain and must still fire.
    #[test]
    fn still_flags_genuine_side_effect_in_cli_file() {
        let src = "\
            import { Command } from 'commander';\n\
            import { globalCache } from './cache';\n\
            globalCache.warmUp();\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/cli.ts");
        assert_eq!(
            diags.len(),
            1,
            "a genuine top-level mutating call must still be flagged, got {diags:?}"
        );
    }

    // Issue #1660: dtslint-style `__tests_dts__/` type-test directories hold
    // type-level assertions where bare calls probe the return type, not runtime
    // side effects. The central `in_test_dir` predicate now classifies them as a
    // test dir, so the rule (reading `ctx.file.path_segments.in_test_dir`) skips
    // them like the existing `dtslint/` / `test-d/` exemptions.
    #[test]
    fn exempts_tests_dts_type_test_dir_issue1660() {
        use crate::files::Language;
        use crate::rules::file_ctx::FileCtx;

        let src = "\
            import { defineConfig, mergeConfig } from '../config';\n\
            const configObjectDefined = defineConfig({});\n\
            defineConfig({ base: '', build: { minify: 'oxc' } });\n\
            mergeConfig({}, {});\n";
        let path = std::path::Path::new("packages/vite/src/node/__tests_dts__/config.ts");
        let project = crate::project::default_static_project_ctx();
        let file = FileCtx::build(path, src, Language::TypeScript, project);
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, src, path, project, &file);
        assert!(
            diags.is_empty(),
            "type-level test calls in __tests_dts__/ must not be flagged, got {diags:?}"
        );
    }

    // Negative-space guard for #1660: the same bare calls in an ordinary source
    // module (not under a type-test dir) are real top-level side effects and
    // must still fire — the exemption is keyed on the directory, not the calls.
    #[test]
    fn still_flags_top_level_calls_outside_tests_dts_dir() {
        use crate::files::Language;
        use crate::rules::file_ctx::FileCtx;

        let src = "\
            import { defineConfig, mergeConfig } from '../config';\n\
            defineConfig({ base: '' });\n\
            mergeConfig({}, {});\n";
        let path = std::path::Path::new("packages/vite/src/node/config.ts");
        let project = crate::project::default_static_project_ctx();
        let file = FileCtx::build(path, src, Language::TypeScript, project);
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, src, path, project, &file);
        assert_eq!(
            diags.len(),
            2,
            "top-level calls in a normal source module must still be flagged, got {diags:?}"
        );
    }

    // Issue #2135: `dayjs.extend(plugin)` / `gsap.registerPlugin(x)` at module
    // scope are one-time library plugin registrations — the documented, required
    // configuration to enable a library feature, not a tree-shaking hazard.
    #[test]
    fn skips_library_plugin_registration() {
        let dayjs = "\
            import dayjs from 'dayjs';\n\
            import duration from 'dayjs/plugin/duration';\n\
            dayjs.extend(duration);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, dayjs, "src/utils.ts");
        assert!(diags.is_empty(), "dayjs.extend(plugin) must be exempt, got {diags:?}");

        let gsap = "\
            import { gsap } from 'gsap';\n\
            import { ScrollTrigger } from 'gsap/ScrollTrigger';\n\
            gsap.registerPlugin(ScrollTrigger);\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, gsap, "src/anim.ts");
        assert!(diags.is_empty(), "gsap.registerPlugin(x) must be exempt, got {diags:?}");
    }

    // Negative-space guard for #2135: a genuine top-level side effect (network
    // I/O, DOM mutation) is NOT a plugin registration and must still flag, even
    // though it is also a member call on an imported binding. `app.use(mw)` is
    // deliberately excluded from the registration set (`use` is far too common),
    // so it must still flag too.
    #[test]
    fn still_flags_non_registration_member_calls() {
        let analytics =
            crate::rules::test_helpers::run_rule(&Check, "analytics.track('load');", "src/a.ts");
        assert_eq!(analytics.len(), 1, "analytics.track must still flag, got {analytics:?}");

        let db = crate::rules::test_helpers::run_rule(&Check, "db.connect();", "src/b.ts");
        assert_eq!(db.len(), 1, "db.connect must still flag, got {db:?}");

        let app = crate::rules::test_helpers::run_rule(&Check, "app.use(mw);", "src/c.ts");
        assert_eq!(app.len(), 1, "app.use(mw) must still flag, got {app:?}");
    }

    // Issue #2083: a top-level `watch(...)` / `debouncedWatch(...)` whose callee
    // is imported from a Vue ecosystem package is the standard Composition API /
    // VueUse mechanism for wiring reactive state at module scope — declarative
    // reactive registration, not a tree-shaking hazard.
    #[test]
    fn skips_vue_reactivity_setup() {
        let composable = "\
            import { ref, watch } from 'vue';\n\
            import { debouncedWatch } from '@vueuse/core';\n\
            export const panelSizes = ref([]);\n\
            watch(panelSizes, (value) => { value.forEach(() => {}); });\n\
            debouncedWatch(panelSizes, (value) => {}, { debounce: 100 });\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, composable, "src/composables/panel.ts");
        assert!(diags.is_empty(), "vue watch/debouncedWatch must be exempt, got {diags:?}");

        let aliased = "\
            import { watchEffect as track } from '@vue/runtime-core';\n\
            track(() => {});\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, aliased, "src/composables/effect.ts");
        assert!(diags.is_empty(), "aliased vue watchEffect must be exempt, got {diags:?}");
    }

    // Negative-space guard for #2083: a `watch(...)` whose callee is NOT imported
    // from a Vue package (a same-named local, or imported from elsewhere) is not
    // the Vue reactivity idiom and must still flag. A genuine side effect at
    // module scope (`fetchData()`, `console.log()`) must also still flag.
    #[test]
    fn still_flags_non_vue_watch_and_side_effects() {
        let local_watch = "\
            function watch(x, cb) { cb(); }\n\
            watch(state, () => {});\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, local_watch, "src/local.ts");
        assert_eq!(diags.len(), 1, "local watch() must still flag, got {diags:?}");

        let other_watch = "\
            import { watch } from 'node:fs';\n\
            watch('./dir', () => {});\n";
        let diags = crate::rules::test_helpers::run_rule(&Check, other_watch, "src/fswatch.ts");
        assert_eq!(diags.len(), 1, "non-vue watch() must still flag, got {diags:?}");

        let fetch = crate::rules::test_helpers::run_rule(&Check, "fetchData();", "src/d.ts");
        assert_eq!(fetch.len(), 1, "fetchData() must still flag, got {fetch:?}");
    }
}
