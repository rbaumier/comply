//! ESLint core rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY, oxlint_and_clippy, oxlint_delegate};

// comply-ignore: max-function-lines — this is a flat data table, not logic; splitting it would scatter related rule entries across files for no readability gain.
pub fn register_all() -> Vec<RuleDef> {
    vec![
        entry(
            "eqeqeq",
            "eqeqeq",
            Severity::Error,
            "Use === over == to avoid type coercion surprises.",
            "Replace `==` with `===` (and `!=` with `!==`). Loose equality \
             triggers implicit coercion rules that hide bugs.",
        ),
        entry(
            "no-var",
            "no-var",
            Severity::Error,
            "Never declare variables with `var`.",
            "Replace `var` with `const` (or `let` only when the binding \
             actually needs to be reassigned).",
        ),
        entry(
            "prefer-const",
            "prefer-const",
            Severity::Error,
            "Prefer `const` over `let` when the binding is never reassigned.",
            "Change `let` to `const` for bindings that are assigned once. \
             The intent becomes explicit and accidental reassignment becomes \
             a compile error.",
        ),
        entry_with_clippy(
            "no-else-return",
            "no-else-return",
            "clippy::redundant_else",
            Severity::Error,
            "Prefer guard clauses over else-after-return.",
            "Remove the `else` after a `return` and de-indent the trailing \
             block. Early returns keep the happy path at the leftmost level.",
        ),
        // `max-params` is handled natively — see `src/rules/max_params/`.
        // The native version exempts fixed-signature library callbacks
        // (TanStack Query `onError`/`queryFn`/etc.) and keeps the same
        // clippy delegation for Rust.
        entry_with_clippy(
            "max-depth",
            "max-depth",
            "clippy::excessive_nesting",
            Severity::Error,
            "Nesting beyond 2 levels is a smell.",
            "Flatten via early return, extract a helper, or invert the \
             condition. Deep nesting hides the happy path.",
        ),
        entry(
            "no-useless-catch",
            "no-useless-catch",
            Severity::Error,
            "A catch that only rethrows is pointless.",
            "If the catch block just rethrows the original error, remove it \
             — the error propagates identically without the ceremony.",
        ),
        // --- v1.1 additions ---
        // `id-length` is handled natively — see `src/rules/id_length/`.
        // `no-await-in-loop` handled natively — see src/rules/no_await_in_loop/.
        entry(
            "no-param-reassign",
            "no-param-reassign",
            Severity::Error,
            "Reassigning function parameters mutates the caller's data.",
            "Copy the argument into a local `let` if you need to mutate it. \
             Mutating params silently surprises callers.",
        ),
        entry(
            "no-empty",
            "no-empty",
            Severity::Error,
            "Empty blocks — including empty `catch` — must be justified.",
            "Either handle the case or add a comment naming why the block \
             is intentionally empty. Silent empty blocks rot into bugs.",
        ),
        // --- Biome-parity additions: rules Biome flags that comply lacked. ---
        entry(
            "no-self-assign",
            "no-self-assign",
            Severity::Error,
            "Assigning a variable to itself does nothing.",
            "Remove the `x = x` assignment or fix the typo — you likely \
             meant a different value or property.",
        ),
        entry(
            "max-nested-callbacks",
            "max-nested-callbacks",
            Severity::Warning,
            "Deeply nested callbacks are unreadable.",
            "Flatten nested callbacks by extracting named functions or \
             switching to async/await. Past three levels the control flow \
             is hard to follow.",
        ),
        entry(
            "no-dupe-keys",
            "no-dupe-keys",
            Severity::Error,
            "Duplicate object keys silently overwrite each other.",
            "Remove or rename the duplicate key. Later keys win, so the \
             earlier value is silently discarded.",
        ),
        entry(
            "no-import-assign",
            "no-import-assign",
            Severity::Error,
            "Imported bindings are read-only.",
            "Don't assign to an imported binding or namespace. Imports are \
             immutable live bindings; assigning to them throws in modules.",
        ),
        entry(
            "no-template-curly-in-string",
            "no-template-curly-in-string",
            Severity::Error,
            "`${...}` in a regular string is not interpolated.",
            "Switch the quotes to backticks to make it a template literal, \
             or remove the `${}` if it was meant literally. In a normal \
             string it stays verbatim text.",
        ),
        entry(
            "no-sequences",
            "no-sequences",
            Severity::Warning,
            "The comma operator hides multiple expressions in one.",
            "Split comma-separated expressions into separate statements. \
             The comma operator evaluates each and returns the last, which \
             surprises most readers.",
        ),
        entry(
            "no-extra-label",
            "no-extra-label",
            Severity::Warning,
            "A label on a loop with no nested target is useless.",
            "Remove the label when `break`/`continue` would target the same \
             loop anyway. Labels only earn their keep when breaking out of \
             an outer loop.",
        ),
        entry(
            "constructor-super",
            "constructor-super",
            Severity::Error,
            "Derived classes must call `super()`; base classes must not.",
            "Add a `super(...)` call in any class that `extends` another, \
             and remove `super()` from classes with no superclass. The \
             wrong shape throws at runtime.",
        ),
        entry(
            "no-setter-return",
            "no-setter-return",
            Severity::Error,
            "A setter's return value is discarded.",
            "Remove the `return value` from the setter — it has no effect. \
             Use a plain method if you need to return something.",
        ),
        entry(
            "no-async-promise-executor",
            "no-async-promise-executor",
            Severity::Error,
            "The `Promise` executor must not be `async`.",
            "Remove `async` from the `new Promise(async (resolve) => ...)` \
             executor. Errors thrown inside an async executor are swallowed \
             and never reject the promise.",
        ),
        entry(
            "no-duplicate-case",
            "no-duplicate-case",
            Severity::Error,
            "Duplicate `case` labels make later branches dead.",
            "Remove or correct the duplicate `case` value. The second \
             identical label can never be reached.",
        ),
        entry(
            "no-div-regex",
            "no-div-regex",
            Severity::Warning,
            "A regex starting with `/=` reads like a division.",
            "Escape the leading equals sign as `/\\=.../`. A bare `/=` at \
             the start of a regex literal is easy to misread as the `/=` \
             operator.",
        ),
        entry(
            "no-lone-blocks",
            "no-lone-blocks",
            Severity::Warning,
            "A standalone block with no block-scoped declarations is dead \
             structure.",
            "Remove the redundant `{ }`. A bare block only matters when it \
             scopes `let`/`const`/`class`; otherwise it just adds \
             indentation.",
        ),
        entry(
            "no-const-assign",
            "no-const-assign",
            Severity::Error,
            "A `const` binding cannot be reassigned.",
            "Reassigning a `const` throws a TypeError at runtime. Declare \
             the variable with `let` if it genuinely needs to change.",
        ),
        entry(
            "no-multi-assign",
            "no-multi-assign",
            Severity::Warning,
            "Chained assignments hide what is being set.",
            "Split `a = b = c` into separate statements. Chained assignment \
             obscures which variables are declared and which are mutated.",
        ),
        entry(
            "curly",
            "curly",
            Severity::Warning,
            "Always wrap control-flow bodies in braces.",
            "Wrap every `if`/`else`/`for`/`while` body in `{ }`, even a \
             single statement. Brace-less bodies silently break the moment \
             a second line is added.",
        ),
        entry(
            "no-console",
            "no-console",
            Severity::Error,
            "`console` calls leak into production.",
            "Remove the `console.*` call or route it through a real logger. \
             Stray console output clutters production logs and can leak \
             data.",
        ),
        entry(
            "no-fallthrough",
            "no-fallthrough",
            Severity::Error,
            "A `switch` case must not silently fall through.",
            "Add a `break`/`return`, or an explicit `// fallthrough` \
             comment when it is intentional. Accidental fallthrough runs \
             the next case's code.",
        ),
        entry(
            "no-self-compare",
            "no-self-compare",
            Severity::Error,
            "Comparing a value with itself is trivially true or false.",
            "Replace `x === x` with the intended operands. The only real \
             use, NaN detection, is clearer as `Number.isNaN(x)`.",
        ),
        entry(
            "no-useless-rename",
            "no-useless-rename",
            Severity::Warning,
            "Renaming a binding to the same name is redundant.",
            "Simplify `{ foo: foo }` to `{ foo }` in imports, exports, and \
             destructuring. Renaming a binding to itself is pure noise.",
        ),
        entry(
            "vars-on-top",
            "vars-on-top",
            Severity::Warning,
            "`var` declarations should sit at the top of their scope.",
            "Move `var` declarations to the top of the function so the code \
             matches their hoisted semantics — or better, switch to \
             `let`/`const`.",
        ),
        entry(
            "max-classes-per-file",
            "max-classes-per-file",
            Severity::Warning,
            "Keep one class per file.",
            "Move extra classes into their own files. One class per file \
             keeps modules focused and imports predictable.",
        ),
        entry(
            "no-multi-str",
            "no-multi-str",
            Severity::Warning,
            "Escaping a newline to span a string is error-prone.",
            "Use a template literal or string concatenation instead of a \
             `\\` line continuation. A trailing space after the backslash \
             silently breaks it.",
        ),
        entry(
            "operator-assignment",
            "operator-assignment",
            Severity::Warning,
            "Prefer compound assignment operators.",
            "Replace `x = x + 1` with `x += 1`, and likewise for the other \
             operators. Shorthand states the intent and avoids repeating \
             the target.",
        ),
        entry(
            "no-constant-binary-expression",
            "no-constant-binary-expression",
            Severity::Error,
            "A binary expression with a constant operand has no effect.",
            "Fix the expression — a constant side means a comparison or \
             short-circuit always resolves the same way, which is usually a \
             typo such as `a === b === c`.",
        ),
        entry(
            "no-func-assign",
            "no-func-assign",
            Severity::Error,
            "A function declaration must not be reassigned.",
            "Don't assign to the name of a `function` declaration. \
             Overwriting it is almost always a bug; use a separate variable \
             instead.",
        ),
        entry(
            "no-irregular-whitespace",
            "no-irregular-whitespace",
            Severity::Error,
            "Irregular whitespace characters break tooling.",
            "Replace non-standard whitespace such as a non-breaking or \
             zero-width space with a regular space. Invisible characters \
             cause confusing parse and diff errors.",
        ),
        entry(
            "no-unassigned-vars",
            "no-unassigned-vars",
            Severity::Error,
            "A variable that is read but never assigned is always \
             undefined.",
            "Assign the variable a value or remove it. A `let`/`var` that \
             is only ever read holds `undefined`, usually a forgotten \
             assignment.",
        ),
        entry(
            "no-empty-pattern",
            "no-empty-pattern",
            Severity::Error,
            "An empty destructuring pattern binds nothing.",
            "Replace the empty `{}`/`[]` with the names you meant to \
             extract, or drop the destructuring. `const {} = x` is almost \
             always a mistake.",
        ),
        entry(
            "no-new",
            "no-new",
            Severity::Error,
            "`new` used only for side effects throws the object away.",
            "Assign the `new X()` result to a variable, or call a plain \
             function if you only want side effects. A bare `new` signals a \
             misused constructor.",
        ),
        entry(
            "no-ex-assign",
            "no-ex-assign",
            Severity::Error,
            "Don't reassign the caught exception.",
            "Use a new local variable instead of assigning to the `catch \
             (e)` binding. Overwriting it loses the original error and \
             confuses debugging.",
        ),
        entry(
            "no-label-var",
            "no-label-var",
            Severity::Error,
            "A label must not share a name with an in-scope variable.",
            "Rename the label or the variable. Reusing the same name makes \
             `break name` ambiguous to readers.",
        ),
        entry(
            "no-unreachable",
            "no-unreachable",
            Severity::Error,
            "Code after a terminating statement never runs.",
            "Remove the unreachable statements after a \
             `return`/`throw`/`break`/`continue`, or fix the control flow \
             that strands them.",
        ),
        entry(
            "no-unused-labels",
            "no-unused-labels",
            Severity::Error,
            "A label that is never referenced is dead.",
            "Remove the unused label. If you meant to jump to it, add the \
             matching `break`/`continue`.",
        ),
        entry(
            "no-class-assign",
            "no-class-assign",
            Severity::Error,
            "A class binding must not be reassigned.",
            "Don't assign to a `class` declaration's name. Reassigning it \
             shadows the class and is almost always a mistake.",
        ),
        entry(
            "no-debugger",
            "no-debugger",
            Severity::Error,
            "`debugger` statements must not be committed.",
            "Remove the `debugger` statement. It halts execution wherever \
             devtools are open and never belongs in shipped code.",
        ),
        entry(
            "no-obj-calls",
            "no-obj-calls",
            Severity::Error,
            "Global namespace objects are not callable.",
            "Don't call `Math()`, `JSON()`, `Reflect()`, and the like — \
             they are namespaces, not functions, so calling them throws a \
             TypeError.",
        ),
        entry(
            "no-this-before-super",
            "no-this-before-super",
            Severity::Error,
            "`this` must not be used before `super()` in a derived \
             constructor.",
            "Move every `this` access after the `super()` call. Touching \
             `this` before the base constructor runs throws a \
             ReferenceError.",
        ),
        entry(
            "yoda",
            "yoda",
            Severity::Warning,
            "Yoda conditions read backwards.",
            "Write `value === 'literal'`, not `'literal' === value`. \
             Putting the variable first reads naturally.",
        ),
        entry(
            "object-shorthand",
            "object-shorthand",
            Severity::Warning,
            "Use shorthand for object properties and methods.",
            "Write `{ foo }` instead of `{ foo: foo }` and `{ method() {} \
             }` instead of `{ method: function () {} }`. Shorthand is the \
             consistent, modern form.",
        ),
        entry(
            "no-alert",
            "no-alert",
            Severity::Error,
            "`alert`, `confirm`, and `prompt` block the UI thread.",
            "Replace `alert`/`confirm`/`prompt` with a non-blocking UI \
             component. These browser dialogs freeze the page and rarely \
             belong in shipped code.",
        ),
        entry(
            "no-extra-boolean-cast",
            "no-extra-boolean-cast",
            Severity::Warning,
            "Redundant boolean casts add noise.",
            "Drop the extra `!!` or `Boolean()` in a context that already \
             coerces to boolean, such as an `if` condition. It is pure \
             noise.",
        ),
        entry(
            "symbol-description",
            "symbol-description",
            Severity::Warning,
            "Every `Symbol()` should carry a description.",
            "Pass a description to `Symbol('...')`. The description is the \
             only thing that identifies a symbol when debugging.",
        ),
        entry(
            "no-compare-neg-zero",
            "no-compare-neg-zero",
            Severity::Error,
            "Comparing against `-0` is misleading.",
            "Replace `x === -0` with `Object.is(x, -0)`. The `===` operator \
             treats `-0` and `0` as equal, so the comparison never does \
             what it looks like.",
        ),
        entry(
            "no-eq-null",
            "no-eq-null",
            Severity::Error,
            "Use `===`/`!==` when comparing with `null`.",
            "Replace `== null` with `=== null`. Loose `== null` quietly \
             matches both `null` and `undefined`; make that intent explicit \
             instead.",
        ),
    ]
}

// Entry-builder helpers used by `register_all` above.

fn entry(
    id: &'static str,
    oxlint_key: &'static str,
    severity: Severity,
    description: &'static str,
    remediation: &'static str,
) -> RuleDef {
    oxlint_delegate(
        RuleMeta {
            id,
            description,
            remediation,
            severity,
            doc_url: None,
            categories: &["typescript"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        oxlint_key,
        TS_FAMILY,
    )
}

/// Same shape as `entry()` but also binds the rule to a clippy lint on Rust.
fn entry_with_clippy(
    id: &'static str,
    oxlint_key: &'static str,
    clippy_lint: &'static str,
    severity: Severity,
    description: &'static str,
    remediation: &'static str,
) -> RuleDef {
    oxlint_and_clippy(
        RuleMeta {
            id,
            description,
            remediation,
            severity,
            doc_url: None,
            categories: &["typescript"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        oxlint_key,
        clippy_lint,
    )
}
