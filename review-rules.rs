/*
- les ally doivent marcher pour jsx, mais aussi tsx, html, vue, etc.
- tous les "See the X spec for the full list" -> s'assurer qu'on a bien la full list
- api-first, auth-on-mutation -> pas sûr que ça soit bien fait/efficace
- colocated-tests -> trop contraignant ?
- rename les noms des dossiers des rules en "-" au lieu de "_" comme separators
- consistent-existence-index-check -> il faut mieux utiliser .includes() que .indexOf() non ? si oui, modifier la règle pour bannir indexOf() (et supprimer index-of-compare-to-positive)
- "group-exports" remediation: "Gather all named exports into a single `export { … }` declaration at the bottom of the file instead of scattering port` across multiple declarations.", -> ça contredit la règle exports-at-top non ? 
- voir les règles jsdoc qui ne sont plus nécessaire quand on fait du ts
- max-union-size pas trop compris la rémédiation, tu peux me donner un exemple ?
- migration-needs-lock-timeout -> est-ce que drizzle ou sqlx ne le font pas déjà pour nous ?
- no-abbreviated-names : il faut trouver un moyen d'avoir un dico commun entre les langages sans copier coller
- no-abusive-eslint-disable -> doit s'appliquer à comply, si on a un commentaire eslint disable avec une regle qui vaut "eslint-disable-no-number" et qu'on a une règle qui vaut "comply-disable-no-number", alors ça doit la désactiver, pareil pour clippy
- no-auth-token-in-localstorage : oui mais si on les set côté front comment on peut faire ?
- no-all-duplicated-branches doublon avec no-duplicated-branches et no-identical-conditions non ?
- améliorer toutes les règles qui se basent sur un tableau de mots pour trouver tous les mots possibles
- no-identical-functions -> meme 80% d'identique devrait trigger non ? ou un % dynamique en fonction de la longueur de la fonction ?
- no-lonely-if -> on a pas déjà une regle comme ça ?
- no-misleading-array-reverse -> on a pas déjà une regle comme ça ?
- no-misleading-collection-name -> on a déjà une regle qui interdit le type dans le nom non (no-type-encoded-names) ?
- no-negation-in-equality-check -> on a déjà une regle non ?
- regarde toutes les regles si il y en a qui ne font pas doublons
- est-ce que tu peux mettre en commentaire d'où provient la règle quand on la récupère d'un plugin eslint ou autre ? (+ un lien vers sa doc serait parfait)
- quand c'est un règle importée, tu peux regarder les tests dans le code original (e.g. les règles eslint) et les ajouter si on ne les a pas, ça permettra de s'assurer que tous les cas sont testés
- no-primitive-wrappers -> on a déjà une regle non ?
- no-put-method -> je suis pas sûr que ce soit une bonne règle ?
- no-raw-db-entity-in-handler -> je suis pas sûr que ce soit une bonne règle ?
- no-redundant-optional -> je crois qu'il y a des règles qui obligent à ne pas avoir d'optional car ça peut poser des problèmes, tu peux chercher et me donner ton avis ?
- no-return-type-any -> on a pas déjà une règle qui ban le any ?
- no-sort-without-comparator -> ça marche sur toSorted aussi ?
- no-unnecessary-array-flat-depth : doublon avec une autre regle non ?
- no-unnecessary-array-splice-count -> peut être interdire array.splice() et préférer une méthode immutable (.filter) ?
- no-unreadable-iife -> ça peut être bien des fois pour faire un block non ? type ```const a = (() => { ... return X; })()
 */ 

--
--
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-object-type-as-default-prop",
    description: "Object/array/function default props create a new reference every render, breaking `React.memo`.",
    remediation: "Move default values to a module-level constant or use `useMemo`/`useCallback`. \
                  `function Foo({ items = DEFAULT_ITEMS })` with `const DEFAULT_ITEMS = []` \
                  outside the component keeps a stable reference.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-complete-sentence",
    description: "JSDoc descriptions must start with a capital letter and end with punctuation.",
    remediation: "Capitalize the first letter and end the description with `.`, `!`, or `?`. Complete sentences read better in generated docs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-block-on-in-async",
    description: "`block_on` from inside `async fn` panics the runtime.",
    remediation: "Replace `runtime.block_on(future)` with `future.await`. \
                  Calling `block_on` while a runtime is already running \
                  triggers tokio's `Cannot start a runtime from within a \
                  runtime` panic.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-box-default",
    description: "`Box::new(T::default())` is `Box::<T>::default()`.",
    remediation: "Replace `Box::new(T::default())` with `Box::<T>::default()`. \
                  The two are equivalent at runtime, but the latter is \
                  one allocation step instead of two and reads as the \
                  obvious idiom. Enforced by `clippy::box_default`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-duplicate-chars",
    description: "Duplicate characters in regex character class are redundant.",
    remediation: "Remove duplicate characters from the `[...]` character class.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-small-switch",
    description: "`switch` with fewer than 3 cases — use `if/else` instead.",
    remediation: "Replace small `switch` statements (< 3 cases) with `if/else` chains. `switch` adds indentation and boilerplate (`break`, `case`, `default`) that isn't justified for 1-2 branches.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-wait-for-selector",
    description: "`page.waitForSelector()` is discouraged — use web-first assertions.",
    remediation: "Replace `waitForSelector` with a locator-based assertion \
                  like `await expect(page.locator(…)).toBeVisible()`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-wait-for-selector.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-on-exported",
    description: "Exported functions must document their public contract.",
    remediation: "Add a `/** ... */` JSDoc block above the export, \
                  describing what the function does, its parameters, and \
                  what it returns. Include an @example when the call site \
                  isn't obvious.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "jsdoc"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-single-call",
    description: "Combine multiple consecutive `.push()`, `.classList.add()`, or `.classList.remove()` into one call.",
    remediation: "Merge consecutive calls to the same method on the same receiver \
                  into a single call with multiple arguments. For example, \
                  `arr.push(a); arr.push(b);` becomes `arr.push(a, b);`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-import-meta-properties",
    description: "Prefer `import.meta.filename` and `import.meta.dirname` over legacy techniques.",
    remediation: "Replace `fileURLToPath(import.meta.url)` with `import.meta.filename` \
                  and `dirname(fileURLToPath(import.meta.url))` with `import.meta.dirname`. \
                  Node.js 21.2+ and Bun support these properties natively.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-starts-ends-with",
    description: "Prefer `String#startsWith()` / `String#endsWith()` over regex `^` / `$` tests.",
    remediation: "Replace `/^pattern/.test(str)` with `str.startsWith('pattern')` and \
                  `/pattern$/.test(str)` with `str.endsWith('pattern')`. \
                  String methods are faster and more readable than regex for simple prefix/suffix checks.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-throws-description",
    description: "Every `@throws` tag must include a description.",
    remediation: "Add a description to the `@throws` tag explaining when/what the function throws.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-throws-description.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-block-on-in-async",
    description: "`block_on` from inside `async fn` panics the runtime.",
    remediation: "Replace `runtime.block_on(future)` with `future.await`. \
                  Calling `block_on` while a runtime is already running \
                  triggers tokio's `Cannot start a runtime from within a \
                  runtime` panic.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-negated-condition",
    description: "Disallow negated conditions with an else branch.",
    remediation: "Swap the if/else branches (or ternary arms) and remove the negation \
                  for clearer intent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "comment-paraphrases-code",
    description: "Comment shares too many tokens with the function name — likely a paraphrase.",
    remediation: "Rewrite the comment to explain WHY the code exists, not WHAT it does. \
                  Name the consequence: what breaks if this line is deleted? If you \
                  can't name a consequence, delete the comment instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
--
pub const META: RuleMeta = RuleMeta {
    id: "db-no-string-concat-sql",
    description: "String concatenation with SQL keywords is a SQL injection vector.",
    remediation: "Use parameterized queries (`$1`, `?`, or ORM methods) instead of string concatenation. Never interpolate user input into SQL strings.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-dangerously-set-inner-html",
    description: "`dangerouslySetInnerHTML` is an XSS vector.",
    remediation: "Remove the dangerouslySetInnerHTML prop. If you must \
                  render HTML, sanitize it with DOMPurify first and add a \
                  comment explaining the content's provenance.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};
--
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-starts-ends-with",
    description: "Prefer `String#startsWith()` / `String#endsWith()` over regex `^` / `$` tests.",
    remediation: "Replace `/^pattern/.test(str)` with `str.startsWith('pattern')` and \
                  `/pattern$/.test(str)` with `str.endsWith('pattern')`. \
                  String methods are faster and more readable than regex for simple prefix/suffix checks.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-assignment",
    description: "Variable is assigned then immediately overwritten.",
    remediation: "Remove the first assignment — it has no observable effect.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-extra-lookaround-assertions",
    description: "Lookaround assertion is useless and can be inlined into the parent pattern.",
    remediation: "Remove the unnecessary lookaround wrapper and inline its contents.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-extra-lookaround-assertions.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-insecure-jwt",
    description: "Weak JWT algorithms (`none`, `HS256`) allow token forgery or trivial brute-force.",
    remediation: "Use asymmetric algorithms (`RS256`, `ES256`) for JWT verification. Never allow `algorithm: 'none'` and avoid `HS256` unless you control both issuer and verifier.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "node-hashbang",
    description: "Files with a hashbang (`#!`) must use the correct format.",
    remediation: "Ensure the hashbang line is `#!/usr/bin/env node` and only present in executable files.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/hashbang.md"),
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-duplicate-chars",
    description: "Duplicate characters in regex character class are redundant.",
    remediation: "Remove duplicate characters from the `[...]` character class.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-bitwise-in-boolean",
    description: "Bitwise operators in boolean contexts are likely typos.",
    remediation: "Use `&&` instead of `&`, `||` instead of `|`. Bitwise operators in `if`/`while` conditions are almost always a mistake.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-comparison-matcher",
    description: "Use built-in comparison matchers instead of comparing manually.",
    remediation: "Replace `expect(a > b).toBe(true)` with \
                  `expect(a).toBeGreaterThan(b)`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-comparison-matcher.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-no-undefined-types",
    description: "JSDoc `@param`/`@returns` type is not a known built-in.",
    remediation: "Fix the type name in the JSDoc tag. Common built-ins: string, number, boolean, Array, Object, Promise, Function, Date, RegExp, Map, Set, Symbol, Error, void, null, undefined, any, never.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-standalone-backslash",
    description: "Backslash followed by a non-special character in regex is an identity escape — likely a mistake.",
    remediation: "Remove the unnecessary backslash or use the correct escape sequence.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-string-literal-v",
    description: "Empty string disjunction in a `v`-flag character class is unexpected and likely a mistake.",
    remediation: "Remove the empty string literal from the character class string disjunction.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-empty-string-literal.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "node-prefer-promises-dns",
    description: "Callback-based `dns.*` methods are discouraged.",
    remediation: "Use `dns.promises.*` or import from `dns/promises` instead of callback-based `dns` methods.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "layer-import-boundary",
    description: "Imports that cross hexagonal architecture layers break \
                  dependency inversion and make the domain untestable.",
    remediation: "Domain must not import from infrastructure or application. \
                  Application must not import from infrastructure. \
                  Use dependency injection or ports/adapters instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["architecture"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-misleading-collection-name",
    description: "Variable name lies about the underlying collection type.",
    remediation: "Rename the binding to match the actual type — `userList` holding \
                  a `Set` becomes `userSet`, `nameMap` holding an `Array` becomes \
                  `nameList`. The name and the type must agree.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["naming"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-yields",
    description: "Generator functions must document yielded values with `@yields`.",
    remediation: "Add a `@yields` tag documenting what the generator yields.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-yields.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-adjacent-overload-signatures",
    description: "Function overload signatures must be consecutive for readability.",
    remediation: "Move all overload signatures for the same function name next to each other.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/adjacent-overload-signatures/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-self-import",
    description: "Module imports itself.",
    remediation: "Remove the self-import. A module should never import from itself — it causes circular dependency issues.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-for-in-iterable",
    description: "`for...in` iterates over object keys, not values — use `for...of` for arrays.",
    remediation: "Replace `for (x in arr)` with `for (x of arr)`. `for...in` enumerates property names (strings), including inherited ones, which is almost never the intent for arrays or iterables.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-error-capture-stack-trace",
    description: "Unnecessary `Error.captureStackTrace()` in Error subclass constructor.",
    remediation: "Remove the `Error.captureStackTrace(this, ClassName)` call. \
                  Built-in Error subclasses already capture the stack trace \
                  automatically via `super()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-event-target",
    description: "Prefer `EventTarget` over `EventEmitter`.",
    remediation: "Use the web-standard `EventTarget` class instead of Node's `EventEmitter` — it works in all runtimes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-redundant-clone",
    description: "Remove `.clone()` calls whose result isn't independently observed.",
    remediation: "Move the value instead of cloning it, or borrow it if the \
                  caller still needs access. Clones allocate and copy — \
                  they're never free. Enable `clippy::redundant_clone`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-unverified-certificate",
    description: "Disabling SSL certificate verification enables man-in-the-middle attacks.",
    remediation: "Remove `rejectUnauthorized: false` and `NODE_TLS_REJECT_UNAUTHORIZED = '0'`. Use proper CA certificates instead.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-object-from-entries",
    description: "Prefer `Object.fromEntries()` over building objects from key-value pairs via `reduce`.",
    remediation: "Use `Object.fromEntries(arr.map(…))` instead of `arr.reduce((acc, …) => ({ ...acc, … }), {})`. It is more readable and avoids quadratic spread copies.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-networkidle",
    description: "`networkidle` is fragile — it waits for no network activity for 500 ms, which is race-prone.",
    remediation: "Replace `networkidle` with a web-first assertion like \
                  `await expect(locator).toBeVisible()` or wait for a \
                  specific response with `page.waitForResponse()`. The \
                  `networkidle` strategy is timing-based and fails on \
                  pages with polling, analytics, or websockets.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-lookaround",
    description: "Empty lookaround (`(?=)`, `(?!)`, `(?<=)`, `(?<!)`) always matches or always fails — likely a mistake.",
    remediation: "Add a pattern inside the lookaround or remove it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "migration-needs-rollback",
    description: "Migration without a `down`/rollback function is irreversible.",
    remediation: "Add an explicit `down()` / `rollback()` function to every migration. Irreversible migrations prevent quick recovery from bad deploys. Make data migrations idempotent with `ON CONFLICT DO NOTHING`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "migrations"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-json-parse-cast",
    description: "`JSON.parse(x) as T` is a lie — validate the runtime shape.",
    remediation: "Replace the cast with runtime validation: \
                  `const parsed = UserSchema.safeParse(JSON.parse(raw))` \
                  (Zod) or a hand-written type guard that inspects the value.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-modern-math-apis",
    description: "Prefer modern `Math` APIs: `Math.hypot()`, `Math.log2()`, `Math.log10()`.",
    remediation: "Replace `Math.sqrt(a*a + b*b)` with `Math.hypot(a, b)`, \
                  `Math.log(x) / Math.LN2` with `Math.log2(x)`, \
                  `Math.log(x) * Math.LOG2E` with `Math.log2(x)`, \
                  `Math.log(x) / Math.LN10` with `Math.log10(x)`, \
                  `Math.log(x) * Math.LOG10E` with `Math.log10(x)`.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-fallback-in-spread",
    description: "Disallow useless fallback when spreading in object literals.",
    remediation: "Remove the `|| {}` or `?? {}` fallback — spreading \
                  `undefined`/`null` is already a no-op in object literals.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-float-for-money",
    description: "`FLOAT`/`DOUBLE`/`REAL` near monetary columns — use `NUMERIC` for money.",
    remediation: "Replace `FLOAT`/`DOUBLE PRECISION`/`REAL` with `NUMERIC(precision, scale)` for any column that holds money, prices, or financial amounts. Floating-point arithmetic introduces rounding errors that compound over transactions.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-named-default",
    description: "Disallow `import { default as foo }` — use `import foo` instead.",
    remediation: "Replace `import { default as foo } from './m'` with \
                  `import foo from './m'`. The named form is verbose and \
                  obscures the intent of importing the default export.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-hex-escape",
    description: "Enforce the use of Unicode escapes instead of hexadecimal escapes.",
    remediation:
        "Replace `\\x41` with `\\u0041` — Unicode escapes are more consistent and readable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-enum-initializers",
    description: "Enum members without explicit values are fragile — reordering changes their runtime value.",
    remediation: "Assign an explicit value to each enum member.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-enum-initializers/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-inferrable-types",
    description: "Explicit types on variables initialized with literals are redundant — TypeScript infers them.",
    remediation: "Remove the type annotation and let TypeScript infer the type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-export-from",
    description: "Prefer `export { x } from './m'` over import-then-re-export.",
    remediation: "Replace `import { x } from './m'; export { x };` with \
                  `export { x } from './m';`. Direct re-export is shorter, \
                  avoids a binding in the local scope, and makes the re-export \
                  intent explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "function-inside-loop",
    description: "Function declaration or expression inside a loop.",
    remediation: "Move the function outside the loop, or use an arrow function if a closure over the loop variable is intended. Declaring functions inside loops creates a new function object on every iteration.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "escape-case",
    description: "Use uppercase characters for the value of escape sequences.",
    remediation: "Replace lowercase hex digits in escape sequences with uppercase: \
                  `\\xff` -> `\\xFF`, `\\u00ff` -> `\\u00FF`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-obscure-range",
    description: "Character class ranges like `[A-z]` include unwanted chars (`[\\]^_\\``). Use `[A-Za-z]` instead.",
    remediation: "Replace obscure ranges with explicit ones: `[A-Za-z]`, `[a-zA-Z0-9]`, etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-complete-sentence",
    description: "JSDoc descriptions must start with a capital letter and end with punctuation.",
    remediation: "Capitalize the first letter and end the description with `.`, `!`, or `?`. Complete sentences read better in generated docs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "assertions-in-tests",
    description: "Test functions must contain at least one assertion.",
    remediation: "Add `expect(...)`, `assert(...)`, `.should(...)`, `.toBe(...)`, `.toEqual(...)`, `.toMatch(...)`, or `.toThrow(...)` to the test body.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-simple-condition-first",
    description: "Prefer simple condition first in logical expressions.",
    remediation: "Swap the operands so the simple condition comes first: \
                  `if (simple && complex())` instead of `if (complex() && simple)`. \
                  Short-circuit evaluation skips the expensive right operand \
                  when the cheap left operand determines the result.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-skipped-test-without-link",
    description: "Every `.skip` must reference a tracked issue.",
    remediation: "Add a comment above the `.skip` with an issue reference \
                  (`#123`, `ABC-456`, or a URL) so the skip can be revived \
                  later. Untracked skips become permanent coverage holes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-redundant-clone",
    description: "Remove `.clone()` calls whose result isn't independently observed.",
    remediation: "Move the value instead of cloning it, or borrow it if the \
                  caller still needs access. Clones allocate and copy — \
                  they're never free. Enable `clippy::redundant_clone`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "newline-after-import",
    description: "Missing blank line after the last import statement.",
    remediation: "Add an empty line between the last import and the first code statement for visual separation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-confusing-quantifier",
    description: "Quantifier is confusing because its minimum is non-zero but the quantified element can match the empty string.",
    remediation: "Replace the quantifier to reflect that it can match the empty string, e.g. use `*` instead of `+`.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/confusing-quantifier.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "db-no-string-concat-sql",
    description: "String concatenation with SQL keywords is a SQL injection vector.",
    remediation: "Use parameterized queries (`$1`, `?`, or ORM methods) instead of string concatenation. Never interpolate user input into SQL strings.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-varchar",
    description: "`VARCHAR(N)` / `CHAR(N)` — use `TEXT` with a CHECK constraint instead.",
    remediation: "Replace `VARCHAR(N)` with `TEXT` + `CHECK(length(col) <= N)`. VARCHAR's length limit provides no performance benefit in PostgreSQL and silently truncates in some contexts.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-object-from-entries",
    description: "Prefer `Object.fromEntries()` over building objects from key-value pairs via `reduce`.",
    remediation: "Use `Object.fromEntries(arr.map(…))` instead of `arr.reduce((acc, …) => ({ ...acc, … }), {})`. It is more readable and avoids quadratic spread copies.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "explicit-length-check",
    description: "Enforce explicitly comparing the `length` or `size` property of a value.",
    remediation: "Use `arr.length > 0` instead of `arr.length` and `arr.length === 0` instead of `!arr.length`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-confidential-logging",
    description: "Logging calls must not contain sensitive data such as passwords, tokens, or API keys.",
    remediation: "Remove or redact sensitive values before logging. Use structured logging with explicit field allow-lists instead of interpolating raw secrets.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-non-literal-fs-filename",
    description: "Filesystem operations with non-literal filenames can lead to path traversal attacks.",
    remediation: "Use string literals for filenames, or validate / sanitize the path before passing it to `fs` methods.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-static-only-class",
    description: "Disallow classes that only have static members.",
    remediation: "Replace the class with plain exported functions or an object \
                  literal. Static-only classes add indirection without benefit \
                  — they cannot be instantiated meaningfully and prevent \
                  tree-shaking.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-insecure-jwt",
    description: "Weak JWT algorithms (`none`, `HS256`) allow token forgery or trivial brute-force.",
    remediation: "Use asymmetric algorithms (`RS256`, `ES256`) for JWT verification. Never allow `algorithm: 'none'` and avoid `HS256` unless you control both issuer and verifier.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-unused-groups",
    description: "Named capturing group is defined but never referenced.",
    remediation: "Use the group via `.groups.name` or `$<name>` in a replacement, or convert to a non-capturing group `(?:...)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-modern-math-apis",
    description: "Prefer modern `Math` APIs: `Math.hypot()`, `Math.log2()`, `Math.log10()`.",
    remediation: "Replace `Math.sqrt(a*a + b*b)` with `Math.hypot(a, b)`, \
                  `Math.log(x) / Math.LN2` with `Math.log2(x)`, \
                  `Math.log(x) * Math.LOG2E` with `Math.log2(x)`, \
                  `Math.log(x) / Math.LN10` with `Math.log10(x)`, \
                  `Math.log(x) * Math.LOG10E` with `Math.log10(x)`.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "package-json-unique-deps",
    description: "A package in both dependencies and devDependencies is ambiguous — \
                  npm/pnpm silently picks one, which surprises consumers.",
    remediation: "Keep each package in exactly one section. Production deps go in \
                  `dependencies`; build-only tools go in `devDependencies`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["package-json"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-unescaped-entities",
    description: "Unescaped `>`, `\"`, `'`, or `}` in JSX text can cause unexpected rendering.",
    remediation: "Replace the character with its HTML entity: `>` with `&gt;`, \
                  `\"` with `&quot;`, `'` with `&apos;`, `}` with `&#125;`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-unescaped-entities.md"),
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-parameter-properties",
    description: "Parameter properties mix declaration and assignment — prefer explicit class properties.",
    remediation: "Declare the property as a class field and assign it in the constructor body.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/parameter-properties/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-no-undefined-types",
    description: "JSDoc `@param`/`@returns` type is not a known built-in.",
    remediation: "Fix the type name in the JSDoc tag. Common built-ins: string, number, boolean, Array, Object, Promise, Function, Date, RegExp, Map, Set, Symbol, Error, void, null, undefined, any, never.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "assertions-in-tests",
    description: "Test functions must contain at least one assertion.",
    remediation: "Add `expect(...)`, `assert(...)`, `.should(...)`, `.toBe(...)`, `.toEqual(...)`, `.toMatch(...)`, or `.toThrow(...)` to the test body.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "hono-cors-permissive",
    description: "Permissive CORS allows any origin to access the API.",
    remediation: "Restrict `cors({ origin: 'https://your-domain.com' })`. Default `cors()` sets `origin: '*'`. With `credentials: true`, the origin must be explicit.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-small-switch",
    description: "`switch` with fewer than 3 cases — use `if/else` instead.",
    remediation: "Replace small `switch` statements (< 3 cases) with `if/else` chains. `switch` adds indentation and boilerplate (`break`, `case`, `default`) that isn't justified for 1-2 branches.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-mutable-exports",
    description: "Mutable export binding (`let`/`var`) — use `const` instead.",
    remediation: "Change `export let` or `export var` to `export const`. Mutable exports are confusing to consumers and hard to reason about.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unverified-hostname",
    description: "Disabling TLS hostname verification allows man-in-the-middle attacks.",
    remediation: "Remove the `checkServerIdentity` override. Setting it to a no-op function or `null` disables hostname verification, making TLS connections vulnerable to MITM attacks.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-self-import",
    description: "Module imports itself.",
    remediation: "Remove the self-import. A module should never import from itself — it causes circular dependency issues.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-consistent-indexed-object-style",
    description: "Prefer `Record<K, V>` over manual index signature `{ [key: K]: V }` for consistency.",
    remediation: "Replace the index signature with `Record<K, V>`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/consistent-indexed-object-style/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-confidential-logging",
    description: "Logging calls must not contain sensitive data such as passwords, tokens, or API keys.",
    remediation: "Remove or redact sensitive values before logging. Use structured logging with explicit field allow-lists instead of interpolating raw secrets.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-misleading-char-class",
    description: "Character class contains multi-codepoint graphemes that will be split.",
    remediation: "Emoji with ZWJ or chars above U+FFFF inside `[...]` are split into individual code points. Use alternation `(?:a|b)` instead of `[ab]` for multi-codepoint sequences.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-post-message-star",
    description: "`postMessage` with `\"*\"` target origin sends messages to any origin.",
    remediation: "Specify an explicit target origin instead of `\"*\"` to prevent cross-origin data leaks.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "new-for-builtins",
    description: "Enforce `new` for constructors and disallow it for `Symbol`/`BigInt`.",
    remediation: "Use `new Map()` instead of `Map()` for constructors that \
                  require it. Conversely, use `Symbol()` and `BigInt()` without \
                  `new` — they are factory functions, not constructors.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-negative-index",
    description: "Prefer negative index over `.length - index` for `slice`, `splice`, `at`, `with`, and related methods.",
    remediation: "Use a negative index directly (e.g. `str.slice(-3)`) instead of computing `.length - N`. Negative indices are shorter and less error-prone.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-on-exported",
    description: "Exported functions must document their public contract.",
    remediation: "Add a `/** ... */` JSDoc block above the export, \
                  describing what the function does, its parameters, and \
                  what it returns. Include an @example when the call site \
                  isn't obvious.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "jsdoc"],
--
pub const META: RuleMeta = RuleMeta {
    id: "numeric-separators-style",
    description: "Enforce the style of numeric separators by correctly grouping digits.",
    remediation:
        "Add underscores to group digits: `1000000` → `1_000_000`, `0xFF00FF` → `0xFF_00_FF`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-regexp-test",
    description: "Prefer `RegExp#test()` over `String#match()` in boolean contexts.",
    remediation: "Use `/pattern/.test(str)` instead of `str.match(/pattern/)` when only a boolean result is needed. `test()` is faster because it stops at the first match.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-mutable-exports",
    description: "Mutable export binding (`let`/`var`) — use `const` instead.",
    remediation: "Change `export let` or `export var` to `export const`. Mutable exports are confusing to consumers and hard to reason about.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-method-this-argument",
    description: "Do not use the `thisArg` parameter in array methods.",
    remediation: "Remove the second argument from the array method call. Use `.bind()` or an arrow function to bind context instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-remove",
    description: "Prefer `childNode.remove()` over `parentNode.removeChild(childNode)`.",
    remediation: "Replace `parent.removeChild(child)` with `child.remove()`. \
                  The modern `.remove()` API is simpler and doesn't require \
                  a reference to the parent node.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-accessor-recursion",
    description: "Disallow recursive access in getters and setters.",
    remediation: "A getter that reads `this.foo` or a setter that writes \
                  `this.foo` on the same property triggers infinite recursion. \
                  Use a backing field (e.g. `this._foo`) or a `WeakMap`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-default-parameters",
    description: "Prefer default parameters over reassignment.",
    remediation: "Replace `x = x || 'default'` / `x = x ?? 'default'` in the \
                  function body with a default parameter value `function f(x = 'default')`. \
                  Default parameters are clearer and avoid subtle bugs with falsy values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-two-nums-quantifier",
    description: "Quantifier `{n,n}` is equivalent to `{n}` — the range is redundant.",
    remediation: "Simplify `{3,3}` to `{3}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "new-for-builtins",
    description: "Enforce `new` for constructors and disallow it for `Symbol`/`BigInt`.",
    remediation: "Use `new Map()` instead of `Map()` for constructors that \
                  require it. Conversely, use `Symbol()` and `BigInt()` without \
                  `new` — they are factory functions, not constructors.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-negated-condition",
    description: "Disallow negated conditions with an else branch.",
    remediation: "Swap the if/else branches (or ternary arms) and remove the negation \
                  for clearer intent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-document-cookie",
    description: "Do not use `document.cookie` directly.",
    remediation: "Use a cookie library (e.g. `js-cookie`, `cookie`) instead of raw `document.cookie` access. Direct cookie manipulation is error-prone and hard to maintain.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-fallback-in-spread",
    description: "Disallow useless fallback when spreading in object literals.",
    remediation: "Remove the `|| {}` or `?? {}` fallback — spreading \
                  `undefined`/`null` is already a no-op in object literals.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-length-check",
    description: "Disallow useless array length check.",
    remediation: "Remove the redundant `.length` guard. `Array#some()` \
                  already returns `false` for an empty array, and \
                  `Array#every()` already returns `true` for an empty array. \
                  The length check adds no value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-named-default",
    description: "Disallow `import { default as foo }` — use `import foo` instead.",
    remediation: "Replace `import { default as foo } from './m'` with \
                  `import foo from './m'`. The named form is verbose and \
                  obscures the intent of importing the default export.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-unverified-certificate",
    description: "Disabling SSL certificate verification enables man-in-the-middle attacks.",
    remediation: "Remove `rejectUnauthorized: false` and `NODE_TLS_REJECT_UNAUTHORIZED = '0'`. Use proper CA certificates instead.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-method-this-argument",
    description: "Do not use the `thisArg` parameter in array methods.",
    remediation: "Remove the second argument from the array method call. Use `.bind()` or an arrow function to bind context instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unnecessary-parameter-property-assignment",
    description: "Assigning `this.x = x` in a constructor is redundant when `x` is already a parameter property.",
    remediation: "Remove the redundant assignment — the parameter property already handles it.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-unnecessary-parameter-property-assignment/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "migration-needs-rollback",
    description: "Migration without a `down`/rollback function is irreversible.",
    remediation: "Add an explicit `down()` / `rollback()` function to every migration. Irreversible migrations prevent quick recovery from bad deploys. Make data migrations idempotent with `ON CONFLICT DO NOTHING`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "migrations"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unassigned-import",
    description: "Side-effect import with no specifiers — assign the import or remove it.",
    remediation: "Import specific bindings (`import { x } from '…'`) or remove the import if the side-effect is unnecessary. CSS/style imports are allowed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-obscure-range",
    description: "Character class ranges like `[A-z]` include unwanted chars (`[\\]^_\\``). Use `[A-Za-z]` instead.",
    remediation: "Replace obscure ranges with explicit ones: `[A-Za-z]`, `[a-zA-Z0-9]`, etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-danger-with-children",
    description: "Using both `dangerouslySetInnerHTML` and `children` on the same element is invalid.",
    remediation: "Use either `dangerouslySetInnerHTML` OR `children`, not both. \
                  React will throw a runtime error when both are provided on \
                  the same element.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};
--

--
pub const META: RuleMeta = RuleMeta {
    id: "no-document-cookie",
    description: "Do not use `document.cookie` directly.",
    remediation: "Use a cookie library (e.g. `js-cookie`, `cookie`) instead of raw `document.cookie` access. Direct cookie manipulation is error-prone and hard to maintain.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-callback-reference",
    description: "Do not pass a function reference directly to an array iterator method.",
    remediation: "Wrap the callback: `.map(x => parseInt(x))` instead of `.map(parseInt)`. Passing a function reference exposes it to unexpected extra arguments (element, index, array).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-static-only-class",
    description: "Disallow classes that only have static members.",
    remediation: "Replace the class with plain exported functions or an object \
                  literal. Static-only classes add indirection without benefit \
                  — they cannot be instantiated meaningfully and prevent \
                  tree-shaking.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-assignment",
    description: "Variable is assigned then immediately overwritten.",
    remediation: "Remove the first assignment — it has no observable effect.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-dangerously-set-inner-html",
    description: "`dangerouslySetInnerHTML` is an XSS vector.",
    remediation: "Remove the dangerouslySetInnerHTML prop. If you must \
                  render HTML, sanitize it with DOMPurify first and add a \
                  comment explaining the content's provenance.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "newline-after-import",
    description: "Missing blank line after the last import statement.",
    remediation: "Add an empty line between the last import and the first code statement for visual separation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-callback-reference",
    description: "Do not pass a function reference directly to an array iterator method.",
    remediation: "Wrap the callback: `.map(x => parseInt(x))` instead of `.map(parseInt)`. Passing a function reference exposes it to unexpected extra arguments (element, index, array).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-raw",
    description: "`String.raw` should be used to avoid escaping `\\`.",
    remediation: "Use `String.raw`\\`...\\`` for strings with multiple backslash escapes. \
                  This is clearer and avoids double-escaping mistakes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unverified-hostname",
    description: "Disabling TLS hostname verification allows man-in-the-middle attacks.",
    remediation: "Remove the `checkServerIdentity` override. Setting it to a no-op function or `null` disables hostname verification, making TLS connections vulnerable to MITM attacks.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-event-target",
    description: "Prefer `EventTarget` over `EventEmitter`.",
    remediation: "Use the web-standard `EventTarget` class instead of Node's `EventEmitter` — it works in all runtimes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-raw",
    description: "`String.raw` should be used to avoid escaping `\\`.",
    remediation: "Use `String.raw`\\`...\\`` for strings with multiple backslash escapes. \
                  This is clearer and avoids double-escaping mistakes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "explicit-length-check",
    description: "Enforce explicitly comparing the `length` or `size` property of a value.",
    remediation: "Use `arr.length > 0` instead of `arr.length` and `arr.length === 0` instead of `!arr.length`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-member-ordering",
    description: "Class and interface members should follow a consistent order: signatures, fields, constructors, methods.",
    remediation: "Re-order members: put signatures first, then fields, then constructors, then methods.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/member-ordering"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-and-conditional-jsx",
    description: "`&&` renders 0/'' when the left operand is falsy-but-not-false.",
    remediation: "Replace `{expr && <X />}` with `{expr ? <X /> : null}` \
                  or `{Boolean(expr) && <X />}`. `&&` lets falsy values \
                  like `0` and `''` leak into the DOM.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-negative-index",
    description: "Prefer negative index over `.length - index` for `slice`, `splice`, `at`, `with`, and related methods.",
    remediation: "Use a negative index directly (e.g. `str.slice(-3)`) instead of computing `.length - N`. Negative indices are shorter and less error-prone.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-commented-out-tests",
    description: "Commented-out tests are dead code that hides missing coverage.",
    remediation: "Remove the commented-out test or re-enable it. Use `.skip()` \
                  if you need to temporarily disable it.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-commented-out-tests.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-and-conditional-jsx",
    description: "`&&` renders 0/'' when the left operand is falsy-but-not-false.",
    remediation: "Replace `{expr && <X />}` with `{expr ? <X /> : null}` \
                  or `{Boolean(expr) && <X />}`. `&&` lets falsy values \
                  like `0` and `''` leak into the DOM.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-sort-without-comparator",
    description: "`.sort()` without comparator sorts lexicographically.",
    remediation: "Pass an explicit comparator: `arr.sort((a, b) => a - b)` for numbers. Default `.sort()` converts to strings, so `[10, 2, 1].sort()` yields `[1, 10, 2]`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "package-json-unique-deps",
    description: "A package in both dependencies and devDependencies is ambiguous — \
                  npm/pnpm silently picks one, which surprises consumers.",
    remediation: "Keep each package in exactly one section. Production deps go in \
                  `dependencies`; build-only tools go in `devDependencies`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["package-json"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-standalone-backslash",
    description: "Backslash followed by a non-special character in regex is an identity escape — likely a mistake.",
    remediation: "Remove the unnecessary backslash or use the correct escape sequence.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-default-parameters",
    description: "Prefer default parameters over reassignment.",
    remediation: "Replace `x = x || 'default'` / `x = x ?? 'default'` in the \
                  function body with a default parameter value `function f(x = 'default')`. \
                  Default parameters are clearer and avoid subtle bugs with falsy values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-inferred-any",
    description: "Detect likely untyped patterns that infer `any`.",
    remediation: "Add an explicit type annotation or use `as T` / `satisfies T` after `JSON.parse()` and `.json()` calls. Avoid `const x: any =` — use a concrete type or `unknown`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-static-mut",
    description: "`static mut` is deprecated and unsafe by design.",
    remediation: "Replace `static mut FOO: T = ...` with a safe \
                  primitive: `OnceLock<T>`/`LazyLock<T>` for \
                  initialize-once values, `Mutex<T>`/`RwLock<T>` for \
                  shared mutable state, or `AtomicU64`/`AtomicBool`/etc \
                  for primitive counters and flags.",
    severity: Severity::Error,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-arc-non-send-sync",
    description: "`Arc<T>` where `T: !Send + !Sync` cannot cross threads.",
    remediation: "Either drop the `Arc` (use `Rc<T>` for single-threaded \
                  sharing) or replace the inner type with a thread-safe \
                  one — `Arc<RefCell<T>>` → `Arc<Mutex<T>>`. Enforced by \
                  `clippy::arc_with_non_send_sync` (correctness, on by default).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-inferred-any",
    description: "Detect likely untyped patterns that infer `any`.",
    remediation: "Add an explicit type annotation or use `as T` / `satisfies T` after `JSON.parse()` and `.json()` calls. Avoid `const x: any =` — use a concrete type or `unknown`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "hono-cors-permissive",
    description: "Permissive CORS allows any origin to access the API.",
    remediation: "Restrict `cors({ origin: 'https://your-domain.com' })`. Default `cors()` sets `origin: '*'`. With `credentials: true`, the origin must be explicit.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-console-spaces",
    description: "Leading/trailing spaces in `console.log` arguments produce misaligned output.",
    remediation: "Remove the leading or trailing space from the string argument. Use comma-separated arguments for spacing instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-panic-macros",
    description: "No `panic!` / `todo!` / `unimplemented!` / `unreachable!` in production.",
    remediation: "Replace the macro with a typed Result error. `todo!()` and \
                  `unimplemented!()` mark placeholders that must not ship. \
                  `unreachable!()` should only mark compiler-proven impossible \
                  states with a `// Impossible: ...` comment. Tests are \
                  exempted — panicking in a `#[test]` is a clean failure.",
    severity: Severity::Error,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "strings-comparison",
    description: "Relational comparison with string literals uses lexicographic order.",
    remediation:
        "Use `localeCompare()` for locale-aware ordering, or compare numeric values explicitly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-timing-attack",
    description: "Direct string comparison of secrets (passwords, tokens, hashes) is vulnerable to timing attacks.",
    remediation: "Use a constant-time comparison function like `crypto.timingSafeEqual()` or `scmp`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-lookaround",
    description: "Empty lookaround (`(?=)`, `(?!)`, `(?<=)`, `(?<!)`) always matches or always fails — likely a mistake.",
    remediation: "Add a pattern inside the lookaround or remove it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-consistent-generic-constructors",
    description: "Generic type arguments should be on the constructor, not the variable annotation.",
    remediation: "Move the type argument from the type annotation to the constructor: `new Map<K, V>()` instead of `const m: Map<K, V> = new Map()`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/consistent-generic-constructors/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-misleading-char-class",
    description: "Character class contains multi-codepoint graphemes that will be split.",
    remediation: "Emoji with ZWJ or chars above U+FFFF inside `[...]` are split into individual code points. Use alternation `(?:a|b)` instead of `[ab]` for multi-codepoint sequences.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-import-meta-properties",
    description: "Prefer `import.meta.filename` and `import.meta.dirname` over legacy techniques.",
    remediation: "Replace `fileURLToPath(import.meta.url)` with `import.meta.filename` \
                  and `dirname(fileURLToPath(import.meta.url))` with `import.meta.dirname`. \
                  Node.js 21.2+ and Bun support these properties natively.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-float-for-money",
    description: "`FLOAT`/`DOUBLE`/`REAL` near monetary columns — use `NUMERIC` for money.",
    remediation: "Replace `FLOAT`/`DOUBLE PRECISION`/`REAL` with `NUMERIC(precision, scale)` for any column that holds money, prices, or financial amounts. Floating-point arithmetic introduces rounding errors that compound over transactions.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-misleading-collection-name",
    description: "Variable name lies about the underlying collection type.",
    remediation: "Rename the binding to match the actual type — `userList` holding \
                  a `Set` becomes `userSet`, `nameMap` holding an `Array` becomes \
                  `nameList`. The name and the type must agree.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["naming"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "max-union-size",
    description: "Union types with more than 5 members are hard to read and maintain.",
    remediation: "Extract the union into a named type alias or reduce the number of members.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-length-check",
    description: "Disallow useless array length check.",
    remediation: "Remove the redundant `.length` guard. `Array#some()` \
                  already returns `false` for an empty array, and \
                  `Array#every()` already returns `true` for an empty array. \
                  The length check adds no value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-tokio-spawn-without-handle",
    description: "`tokio::spawn(..)` whose JoinHandle is dropped silently swallows panics.",
    remediation: "Capture the JoinHandle and `.await` it, or pass the \
                  task through a wrapper like `tokio::spawn(async { \
                  if let Err(e) = work().await { tracing::error!(?e); } \
                  })`. Fire-and-forget loses every error and every panic.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "numeric-separators-style",
    description: "Enforce the style of numeric separators by correctly grouping digits.",
    remediation:
        "Add underscores to group digits: `1000000` → `1_000_000`, `0xFF00FF` → `0xFF_00_FF`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-danger-with-children",
    description: "Using both `dangerouslySetInnerHTML` and `children` on the same element is invalid.",
    remediation: "Use either `dangerouslySetInnerHTML` OR `children`, not both. \
                  React will throw a runtime error when both are provided on \
                  the same element.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-undefined",
    description: "Disallow useless `undefined`.",
    remediation: "Remove the explicit `undefined` — JavaScript already defaults \
                  to it in `return`, `let`/`var` initializers, and default parameter values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-single-char-class",
    description: "Character class with a single character is unnecessary.",
    remediation: "Replace `[x]` with `x` (or `\\.` for `[.]`). Single-character classes add visual noise without changing semantics.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-yields-description",
    description: "Every `@yields` tag must include a description.",
    remediation: "Add a description to the `@yields` tag explaining what the generator yields.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-yields-description.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-timing-attack",
    description: "Direct string comparison of secrets (passwords, tokens, hashes) is vulnerable to timing attacks.",
    remediation: "Use a constant-time comparison function like `crypto.timingSafeEqual()` or `scmp`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-hyphen-before-param-description",
    description: "`@param` descriptions must be preceded by a hyphen.",
    remediation: "Insert a ` - ` between the parameter name and its description: `@param name - description`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-hyphen-before-param-description.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-return-type-any",
    description: "Functions with explicit `: any` return type defeat type safety.",
    remediation: "Replace `: any` with a specific return type or use `unknown` if the type is truly dynamic.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-switch-case",
    description: "Disallow useless case in switch statements.",
    remediation: "Remove the empty case that falls through to `default` — \
                  it has no effect since `default` already handles all \
                  unmatched values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-networkidle",
    description: "`networkidle` is fragile — it waits for no network activity for 500 ms, which is race-prone.",
    remediation: "Replace `networkidle` with a web-first assertion like \
                  `await expect(locator).toBeVisible()` or wait for a \
                  specific response with `page.waitForResponse()`. The \
                  `networkidle` strategy is timing-based and fails on \
                  pages with polling, analytics, or websockets.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "no-non-literal-fs-filename",
    description: "Filesystem operations with non-literal filenames can lead to path traversal attacks.",
    remediation: "Use string literals for filenames, or validate / sanitize the path before passing it to `fs` methods.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "vue-no-duplicate-v-if",
    description: "Two opposite `v-if` conditions should be `v-if`/`v-else`.",
    remediation: "Replace `v-if=\"x\"` + `v-if=\"!x\"` with `v-if=\"x\"` / `v-else`. \
                  Two separate `v-if` directives evaluate independently — if the \
                  condition changes between the two evaluations, both render or \
                  neither does.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "vue"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-panic-macros",
    description: "No `panic!` / `todo!` / `unimplemented!` / `unreachable!` in production.",
    remediation: "Replace the macro with a typed Result error. `todo!()` and \
                  `unimplemented!()` mark placeholders that must not ship. \
                  `unreachable!()` should only mark compiler-proven impossible \
                  states with a `// Impossible: ...` comment. Tests are \
                  exempted — panicking in a `#[test]` is a clean failure.",
    severity: Severity::Error,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-tokio-spawn-without-handle",
    description: "`tokio::spawn(..)` whose JoinHandle is dropped silently swallows panics.",
    remediation: "Capture the JoinHandle and `.await` it, or pass the \
                  task through a wrapper like `tokio::spawn(async { \
                  if let Err(e) = work().await { tracing::error!(?e); } \
                  })`. Fire-and-forget loses every error and every panic.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-hex-escape",
    description: "Enforce the use of Unicode escapes instead of hexadecimal escapes.",
    remediation:
        "Replace `\\x41` with `\\u0041` — Unicode escapes are more consistent and readable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "comment-paraphrases-code",
    description: "Comment shares too many tokens with the function name — likely a paraphrase.",
    remediation: "Rewrite the comment to explain WHY the code exists, not WHAT it does. \
                  Name the consequence: what breaks if this line is deleted? If you \
                  can't name a consequence, delete the comment instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-accessor-recursion",
    description: "Disallow recursive access in getters and setters.",
    remediation: "A getter that reads `this.foo` or a setter that writes \
                  `this.foo` on the same property triggers infinite recursion. \
                  Use a backing field (e.g. `this._foo`) or a `WeakMap`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-empty-test-file",
    description: "Test file contains no test assertions — dead weight in the test suite.",
    remediation: "Add test cases or remove the file. A test file without `test(`, `it(`, `describe(`, or `expect(` provides no value and clutters the test suite.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "layer-import-boundary",
    description: "Imports that cross hexagonal architecture layers break \
                  dependency inversion and make the domain untestable.",
    remediation: "Domain must not import from infrastructure or application. \
                  Application must not import from infrastructure. \
                  Use dependency injection or ports/adapters instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["architecture"],
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-single-char-class",
    description: "Character class with a single character is unnecessary.",
    remediation: "Replace `[x]` with `x` (or `\\.` for `[.]`). Single-character classes add visual noise without changing semantics.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-code-point",
    description: "Prefer `String#codePointAt()` over `String#charCodeAt()` and `String.fromCodePoint()` over `String.fromCharCode()`.",
    remediation: "Use `codePointAt()` instead of `charCodeAt()` and `String.fromCodePoint()` instead of `String.fromCharCode()`. The code-point variants handle full Unicode (including astral symbols) correctly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-export-from",
    description: "Prefer `export { x } from './m'` over import-then-re-export.",
    remediation: "Replace `import { x } from './m'; export { x };` with \
                  `export { x } from './m';`. Direct re-export is shorter, \
                  avoids a binding in the local scope, and makes the re-export \
                  intent explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-bitwise-in-boolean",
    description: "Bitwise operators in boolean contexts are likely typos.",
    remediation: "Use `&&` instead of `&`, `||` instead of `|`. Bitwise operators in `if`/`while` conditions are almost always a mistake.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-error-capture-stack-trace",
    description: "Unnecessary `Error.captureStackTrace()` in Error subclass constructor.",
    remediation: "Remove the `Error.captureStackTrace(this, ClassName)` call. \
                  Built-in Error subclasses already capture the stack trace \
                  automatically via `super()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-remove",
    description: "Prefer `childNode.remove()` over `parentNode.removeChild(childNode)`.",
    remediation: "Replace `parent.removeChild(child)` with `child.remove()`. \
                  The modern `.remove()` API is simpler and doesn't require \
                  a reference to the parent node.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-top-level-await",
    description: "Prefer top-level await over async IIFE or async-function-then-call patterns.",
    remediation: "Use top-level `await` directly instead of wrapping in an async IIFE \
                  or defining an async function and immediately calling it. \
                  Top-level await is supported in ESM.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "vue-no-duplicate-v-if",
    description: "Two opposite `v-if` conditions should be `v-if`/`v-else`.",
    remediation: "Replace `v-if=\"x\"` + `v-if=\"!x\"` with `v-if=\"x\"` / `v-else`. \
                  Two separate `v-if` directives evaluate independently — if the \
                  condition changes between the two evaluations, both render or \
                  neither does.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "vue"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-immediate-return",
    description: "Variable is assigned and immediately returned.",
    remediation: "Return the expression directly: `return computeValue()` instead of `const result = computeValue(); return result;`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-simple-condition-first",
    description: "Prefer simple condition first in logical expressions.",
    remediation: "Swap the operands so the simple condition comes first: \
                  `if (simple && complex())` instead of `if (complex() && simple)`. \
                  Short-circuit evaluation skips the expensive right operand \
                  when the cheap left operand determines the result.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-undefined-assignment",
    description: "Assigning `undefined` explicitly is unnecessary.",
    remediation: "Use `let x;` instead of `let x = undefined;`, or use `delete obj.prop` instead of `obj.prop = undefined`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-dataset",
    description: "Prefer `.dataset` over `.setAttribute('data-*')` / `.getAttribute('data-*')`.",
    remediation: "Replace `.setAttribute('data-foo', v)` with `.dataset.foo = v` and \
                  `.getAttribute('data-foo')` with `.dataset.foo`. The `dataset` API \
                  is cleaner and avoids string-based attribute manipulation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-code-point",
    description: "Prefer `String#codePointAt()` over `String#charCodeAt()` and `String.fromCodePoint()` over `String.fromCharCode()`.",
    remediation: "Use `codePointAt()` instead of `charCodeAt()` and `String.fromCodePoint()` instead of `String.fromCharCode()`. The code-point variants handle full Unicode (including astral symbols) correctly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-sort-mutation",
    description: "Prefer `Array#toSorted()` over `Array#sort()` (mutates in place).",
    remediation: "Replace `.sort()` with `.toSorted()`. `Array#sort()` mutates the \
                  array in place which can cause subtle bugs. `Array#toSorted()` \
                  returns a new sorted array, leaving the original unchanged.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-lossy-as-cast",
    description: "`as` casts that can truncate or lose precision are silent bugs.",
    remediation: "Replace the `as` cast with `try_into()` (returns Result) \
                  or `u8::try_from(x)` for integer narrowing. For \
                  guaranteed-safe widening casts (`u8` → `u32`), use \
                  `From::from(x)` / `x.into()` instead — explicit, \
                  documents the conversion is total.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "template-indent",
    description: "Template literals should not inherit indentation from surrounding code.",
    remediation: "Strip the common leading whitespace from the template literal \
                  content, or use a dedent/stripIndent helper.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-keyword-prefix",
    description: "Do not prefix identifiers with keyword `new` or `class`.",
    remediation: "Rename the identifier to remove the keyword prefix. \
                  For example, `newUser` -> `user`, `classNames` -> `names` or `cssNames`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-return-type-any",
    description: "Functions with explicit `: any` return type defeat type safety.",
    remediation: "Replace `: any` with a specific return type or use `unknown` if the type is truly dynamic.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-template",
    description: "Generic functions and types must document type parameters with `@template`.",
    remediation: "Add a `@template` tag for each type parameter in the signature.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-template.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-consistent-type-definitions",
    description: "Enforce consistent use of `interface` or `type` for object type definitions.",
    remediation: "Use `interface` for object shapes (default), or use `type` consistently.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/consistent-type-definitions/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-builder-without-must-use",
    description: "Builder types need `#[must_use]` to catch forgotten `.build()` calls.",
    remediation: "Add `#[must_use]` above the struct definition. Without \
                  it, callers who forget the final `.build()` get a silent \
                  no-op instead of a compiler warning.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-unassigned-import",
    description: "Side-effect import with no specifiers — assign the import or remove it.",
    remediation: "Import specific bindings (`import { x } from '…'`) or remove the import if the side-effect is unnecessary. CSS/style imports are allowed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-bigint-literals",
    description: "Prefer `BigInt` literals over `BigInt(…)` constructor.",
    remediation: "Replace `BigInt(123)` with `123n` — the literal form is shorter and clearer.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-array-key",
    description: "TanStack Query keys must be arrays, not strings.",
    remediation: "Wrap the string in brackets: `queryKey: ['todos']`. \
                  v5 requires arrays, and hierarchical invalidation \
                  (`invalidateQueries({ queryKey: ['todos'] })`) only \
                  works on array keys.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "tanstack"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-increment",
    description: "`return x++` / `return x--` returns the value *before* the increment.",
    remediation: "Increment before the return (`x++; return x;`) or use prefix (`return ++x`).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-empty-test-file",
    description: "Test file contains no test assertions — dead weight in the test suite.",
    remediation: "Add test cases or remove the file. A test file without `test(`, `it(`, `describe(`, or `expect(` provides no value and clutters the test suite.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unused-vars",
    description: "Declared variables that are never used are dead code.",
    remediation: "Remove the unused variable or prefix with `_` to indicate intentional non-use.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-unused-vars"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-check-param-names",
    description: "JSDoc `@param` names must match actual function parameters.",
    remediation: "Update the `@param` tag name to match the function signature. Stale or mismatched param docs mislead callers.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-sort-mutation",
    description: "Prefer `Array#toSorted()` over `Array#sort()` (mutates in place).",
    remediation: "Replace `.sort()` with `.toSorted()`. `Array#sort()` mutates the \
                  array in place which can cause subtle bugs. `Array#toSorted()` \
                  returns a new sorted array, leaving the original unchanged.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-force-option",
    description: "`force: true` bypasses Playwright's actionability checks, hiding real UI issues.",
    remediation: "Remove `force: true` from the action options. If the \
                  element is not actionable, fix the underlying page state \
                  instead of bypassing the check — forcing clicks masks \
                  real accessibility and timing bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-increment",
    description: "`return x++` / `return x--` returns the value *before* the increment.",
    remediation: "Increment before the return (`x++; return x;`) or use prefix (`return ++x`).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-escape-backspace",
    description: "`[\\b]` in a regex matches the backspace character, not a word boundary — this is almost always a mistake.",
    remediation: "Use `\\b` outside a character class for a word boundary. If you truly need backspace, add a comment explaining the intent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-conditional-in-test",
    description: "Conditional logic in tests makes them non-deterministic.",
    remediation: "Remove `if`/`switch`/ternary from the test body. Write \
                  separate tests for each branch.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-conditional-in-test.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-sort-without-comparator",
    description: "`.sort()` without comparator sorts lexicographically.",
    remediation: "Pass an explicit comparator: `arr.sort((a, b) => a - b)` for numbers. Default `.sort()` converts to strings, so `[10, 2, 1].sort()` yields `[1, 10, 2]`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "law-of-demeter",
    description: "Chained member access couples the caller to the entire object graph.",
    remediation: "Add a direct accessor on the immediate dependency. \
                  `order.getCustomer().getAddress().getCity()` → expose \
                  `order.shippingCity()`. The caller shouldn't know how \
                  Customer and Address are structured.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-read-only-props",
    description: "React component props should be wrapped in `Readonly<>`.",
    remediation: "Wrap the props type: `(props: Readonly<MyType>)` or `({ x }: Readonly<MyType>)`. This prevents accidental mutation of props.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-immediate-return",
    description: "Variable is assigned and immediately returned.",
    remediation: "Return the expression directly: `return computeValue()` instead of `const result = computeValue(); return result;`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-namespace-import",
    description: "Namespace import (`import * as`) — prefer named imports.",
    remediation: "Replace `import * as X from 'y'` with named imports `import { a, b } from 'y'`. Namespace imports defeat tree-shaking and obscure the actual API surface.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "non-existent-operator",
    description: "Typo operator detected — `=+`, `=-`, `=!` are not valid operators.",
    remediation: "Swap the characters: `=+` → `+=`, `=-` → `-=`, `=!` → `!=`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-force-option",
    description: "`force: true` bypasses Playwright's actionability checks, hiding real UI issues.",
    remediation: "Remove `force: true` from the action options. If the \
                  element is not actionable, fix the underlying page state \
                  instead of bypassing the check — forcing clicks masks \
                  real accessibility and timing bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "sql-prefer-exists-over-in",
    description: "`WHERE x IN (SELECT ...)` — prefer `EXISTS` which exits on first match.",
    remediation: "Replace `WHERE col IN (SELECT ...)` with `WHERE EXISTS (SELECT 1 FROM ... WHERE ...)`. EXISTS short-circuits on the first match; IN must materialize the entire subquery.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-match-snapshot",
    description: "Snapshot assertions are a maintenance trap.",
    remediation: "Replace `toMatchSnapshot()` with specific assertions on \
                  the fields that matter. Snapshots break on unrelated \
                  refactors and get blindly updated, losing all assertion \
                  value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "template-indent",
    description: "Template literals should not inherit indentation from surrounding code.",
    remediation: "Strip the common leading whitespace from the template literal \
                  content, or use a dedent/stripIndent helper.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-unsafe-impl-without-comment",
    description: "`unsafe impl` requires a `// SAFETY:` comment.",
    remediation: "Add a `// SAFETY: ...` comment immediately above the \
                  `unsafe impl` block. Spell out which invariants of \
                  the unsafe trait the type upholds — without it, the \
                  contract is unauditable.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "law-of-demeter",
    description: "Chained member access couples the caller to the entire object graph.",
    remediation: "Add a direct accessor on the immediate dependency. \
                  `order.getCustomer().getAddress().getCity()` → expose \
                  `order.shippingCity()`. The caller shouldn't know how \
                  Customer and Address are structured.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
--
pub const META: RuleMeta = RuleMeta {
    id: "error-without-cause",
    description: "new Error(e.message) drops the original stack — pass { cause: e }.",
    remediation: "When wrapping a caught error, preserve the original stack and chain: \
                  `throw new Error('high-level message', { cause: original })`. \
                  Without `cause`, the debugger sees the wrapped message but loses \
                  the source location, type, and nested cause chain.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-arc-non-send-sync",
    description: "`Arc<T>` where `T: !Send + !Sync` cannot cross threads.",
    remediation: "Either drop the `Arc` (use `Rc<T>` for single-threaded \
                  sharing) or replace the inner type with a thread-safe \
                  one — `Arc<RefCell<T>>` → `Arc<Mutex<T>>`. Enforced by \
                  `clippy::arc_with_non_send_sync` (correctness, on by default).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--

--
pub const META: RuleMeta = RuleMeta {
    id: "non-existent-operator",
    description: "Typo operator detected — `=+`, `=-`, `=!` are not valid operators.",
    remediation: "Swap the characters: `=+` → `+=`, `=-` → `-=`, `=!` → `!=`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-bigint-literals",
    description: "Prefer `BigInt` literals over `BigInt(…)` constructor.",
    remediation: "Replace `BigInt(123)` with `123n` — the literal form is shorter and clearer.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-use-state-lazy-init",
    description: "`useState(expensive())` runs on every render.",
    remediation: "Wrap the initializer in a lazy function: \
                  `useState(() => expensive())`. Passing a function means \
                  React only calls it once on mount. Bare expressions run \
                  every render and crash in SSR for browser APIs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
--

--
--
pub const META: RuleMeta = RuleMeta {
    id: "no-weak-keys",
    description: "Weak cryptographic key lengths are vulnerable to brute-force attacks.",
    remediation: "Use RSA >= 2048 bits and EC >= P-256. Prefer Ed25519 or P-384 for new keys.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-nested-step",
    description: "Nested `test.step()` calls make test flow hard to follow.",
    remediation: "Flatten steps so they are sequential instead of nested.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-nested-step.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-read-only-props",
    description: "React component props should be wrapped in `Readonly<>`.",
    remediation: "Wrap the props type: `(props: Readonly<MyType>)` or `({ x }: Readonly<MyType>)`. This prevents accidental mutation of props.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-magic-numbers",
    description: "Magic numbers make code harder to understand — use named constants instead.",
    remediation: "Extract the number into a named `const`. TS enums, numeric literal types, `readonly` properties, and common values (0, 1, -1) are allowed.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-magic-numbers"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-builder-without-must-use",
    description: "Builder types need `#[must_use]` to catch forgotten `.build()` calls.",
    remediation: "Add `#[must_use]` above the struct definition. Without \
                  it, callers who forget the final `.build()` get a silent \
                  no-op instead of a compiler warning.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-spread",
    description: "Disallow unnecessary spread.",
    remediation: "Remove the redundant spread — `[...[1,2]]` is just `[1,2]` \
                  and `{...{a:1}}` is just `{a:1}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-set-x-to-y",
    description: "Function names like setStatusToClosed encode implementation, not intent.",
    remediation: "Rename to express the INTENT, not the storage operation: \
                  `setStatusToClosed` → `closeAccount`, `setRoleToAdmin` → `promoteToAdmin`. \
                  Callers should read like a story, not a database update.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["naming"],
--
pub const META: RuleMeta = RuleMeta {
    id: "strings-comparison",
    description: "Relational comparison with string literals uses lexicographic order.",
    remediation:
        "Use `localeCompare()` for locale-aware ordering, or compare numeric values explicitly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "sql-prefer-exists-over-in",
    description: "`WHERE x IN (SELECT ...)` — prefer `EXISTS` which exits on first match.",
    remediation: "Replace `WHERE col IN (SELECT ...)` with `WHERE EXISTS (SELECT 1 FROM ... WHERE ...)`. EXISTS short-circuits on the first match; IN must materialize the entire subquery.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-page-pause",
    description: "`page.pause()` is a debug-only API that halts test execution.",
    remediation: "Remove `page.pause()`. It opens the Playwright Inspector \
                  and blocks execution indefinitely — CI will hang until it \
                  times out.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-escape-backspace",
    description: "`[\\b]` in a regex matches the backspace character, not a word boundary — this is almost always a mistake.",
    remediation: "Use `\\b` outside a character class for a word boundary. If you truly need backspace, add a comment explaining the intent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-key",
    description: "Missing `key` prop inside iterator — React needs stable keys to reconcile lists.",
    remediation: "Add a unique, stable `key` prop to each JSX element returned \
                  from `.map()`, `.flatMap()`, `.from()`, or an array literal.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/jsx-key.md"),
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "max-union-size",
    description: "Union types with more than 5 members are hard to read and maintain.",
    remediation: "Extract the union into a named type alias or reduce the number of members.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-use-state-lazy-init",
    description: "`useState(expensive())` runs on every render.",
    remediation: "Wrap the initializer in a lazy function: \
                  `useState(() => expensive())`. Passing a function means \
                  React only calls it once on mount. Bare expressions run \
                  every render and crash in SSR for browser APIs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-json-parse-cast",
    description: "`JSON.parse(x) as T` is a lie — validate the runtime shape.",
    remediation: "Replace the cast with runtime validation: \
                  `const parsed = UserSchema.safeParse(JSON.parse(raw))` \
                  (Zod) or a hand-written type guard that inspects the value.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-match-snapshot",
    description: "Snapshot assertions are a maintenance trap.",
    remediation: "Replace `toMatchSnapshot()` with specific assertions on \
                  the fields that matter. Snapshots break on unrelated \
                  refactors and get blindly updated, losing all assertion \
                  value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "package-json-sorted-deps",
    description: "Unsorted dependencies in package.json cause needless merge conflicts.",
    remediation: "Sort dependency keys alphabetically in each section \
                  (dependencies, devDependencies, peerDependencies).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["package-json"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-potentially-useless-backreference",
    description: "Backreference may be useless because some paths to it do not go through the referenced group.",
    remediation: "Restructure the regex so all paths to the backreference pass through the referenced capturing group.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-potentially-useless-backreference.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-undefined",
    description: "Disallow useless `undefined`.",
    remediation: "Remove the explicit `undefined` — JavaScript already defaults \
                  to it in `return`, `let`/`var` initializers, and default parameter values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-namespace-import",
    description: "Namespace import (`import * as`) — prefer named imports.",
    remediation: "Replace `import * as X from 'y'` with named imports `import { a, b } from 'y'`. Namespace imports defeat tree-shaking and obscure the actual API surface.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-constructor-side-effects",
    description: "`new X()` without assignment is a side-effect anti-pattern.",
    remediation: "Assign the result of `new X()` to a variable, or refactor side effects out of the constructor into a static method.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "package-json-sorted-deps",
    description: "Unsorted dependencies in package.json cause needless merge conflicts.",
    remediation: "Sort dependency keys alphabetically in each section \
                  (dependencies, devDependencies, peerDependencies).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["package-json"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-inferrable-types",
    description: "Explicit types on variables initialized with literals are redundant — TypeScript infers them.",
    remediation: "Remove the type annotation and let TypeScript infer the type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "hono-cookie-no-samesite",
    description: "Cookie without `sameSite` or with `sameSite: 'None'` — vulnerable to CSRF.",
    remediation: "Set `sameSite: 'Lax'` (default for most cases) or `sameSite: 'Strict'` for sensitive cookies.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "hono"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-switch-case",
    description: "Disallow useless case in switch statements.",
    remediation: "Remove the empty case that falls through to `default` — \
                  it has no effect since `default` already handles all \
                  unmatched values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "hono-cookie-no-samesite",
    description: "Cookie without `sameSite` or with `sameSite: 'None'` — vulnerable to CSRF.",
    remediation: "Set `sameSite: 'Lax'` (default for most cases) or `sameSite: 'Strict'` for sensitive cookies.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "hono"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-thenable",
    description: "Disallow `then` property on objects and classes.",
    remediation: "Rename the `then` method/property. Objects with a `then` \
                  method are treated as thenables by `await` and \
                  `Promise.resolve()`, causing unexpected behavior.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-optional-catch-binding",
    description: "Prefer omitting the `catch` binding parameter when it is unused.",
    remediation: "Remove the unused catch binding: use `catch { … }` instead of \
                  `catch (error) { … }`. Optional catch binding is supported in ES2019+.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-character-class",
    description: "Empty character class `[]` matches nothing and is likely a mistake.",
    remediation: "Remove the empty `[]` or add characters inside the brackets.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-top-level-await",
    description: "Prefer top-level await over async IIFE or async-function-then-call patterns.",
    remediation: "Use top-level `await` directly instead of wrapping in an async IIFE \
                  or defining an async function and immediately calling it. \
                  Top-level await is supported in ESM.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "error-message-is-remediation",
    description: "Error messages should describe what went wrong and what to do about it.",
    remediation: "Replace short/noun-only error messages like `\"Invalid\"` or `\"Not found\"` with actionable messages: `\"User not found — verify the ID and retry\"`. Good errors contain a verb and guide the reader toward a fix.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-character-class",
    description: "Empty character class `[]` matches nothing and is likely a mistake.",
    remediation: "Remove the empty `[]` or add characters inside the brackets.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unreadable-iife",
    description: "IIFE with parenthesized arrow function body is unreadable.",
    remediation: "Extract the inner expression from the arrow function body \
                  into a variable, or remove the unnecessary parentheses \
                  around the body.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "imports-first",
    description: "Import statements must appear before any other code.",
    remediation: "Move all import/require statements to the top of the file, before any non-import code (except directives like `'use strict'`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-associative-arrays",
    description: "Arrays should not be used as associative arrays (use Map or object instead).",
    remediation: "Use `Map<string, T>` or a plain object `Record<string, T>` instead of assigning string keys on an array.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-lossy-as-cast",
    description: "`as` casts that can truncate or lose precision are silent bugs.",
    remediation: "Replace the `as` cast with `try_into()` (returns Result) \
                  or `u8::try_from(x)` for integer narrowing. For \
                  guaranteed-safe widening casts (`u8` → `u32`), use \
                  `From::from(x)` / `x.into()` instead — explicit, \
                  documents the conversion is total.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "no-associative-arrays",
    description: "Arrays should not be used as associative arrays (use Map or object instead).",
    remediation: "Use `Map<string, T>` or a plain object `Record<string, T>` instead of assigning string keys on an array.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "comma-or-logical-or-case",
    description: "Switch `case` uses comma or `||` instead of fall-through.",
    remediation:
        "Use separate `case` clauses with fall-through instead of comma or `||` in a single `case`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-rc-mutex",
    description: "`Rc<Mutex<T>>` pays the Mutex cost for zero benefit — Rc is !Send.",
    remediation: "Replace `Rc<Mutex<T>>` with `Rc<RefCell<T>>` (single-threaded \
                  interior mutability, no atomic ops). If you actually need \
                  cross-thread sharing, use `Arc<Mutex<T>>` instead.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-flag",
    description: "Regex flag has no effect because the pattern does not contain anything that would be affected by it.",
    remediation: "Remove the unnecessary flag from the regex.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-useless-flag.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-collection-size-mischeck",
    description: "`.length >= 0` is always true; `.length < 0` is always false.",
    remediation: "Use `.length > 0` to check non-empty, or `.length === 0` to check empty.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-undefined-assignment",
    description: "Assigning `undefined` explicitly is unnecessary.",
    remediation: "Use `let x;` instead of `let x = undefined;`, or use `delete obj.prop` instead of `obj.prop = undefined`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "blank-line-between-blocks",
    description: "Missing blank lines between logical blocks.",
    remediation: "Add a blank line before `return` statements (unless preceded by `}`) and between `const`/`let` declaration groups and function calls. Visual separation improves scannability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-collection-size-mischeck",
    description: "`.length >= 0` is always true; `.length < 0` is always false.",
    remediation: "Use `.length > 0` to check non-empty, or `.length === 0` to check empty.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-double-cast",
    description: "Double casts `as X as Y` hide misaligned types.",
    remediation: "Remove the double cast and fix the real misalignment. \
                  Either align the producer's type with the consumer's, \
                  or validate the value at the boundary using a type guard \
                  or Zod schema that actually checks the runtime shape.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
--
pub const META: RuleMeta = RuleMeta {
    id: "too-many-break-or-continue",
    description: "Loop contains 2+ `break`/`continue` statements — consider refactoring.",
    remediation: "Extract the loop body into a function, use early returns, or restructure the logic. Multiple break/continue statements make loops hard to follow and often indicate the loop is doing too much.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "switch-case-break-position",
    description: "`break`/`return` should be inside the case block, not after it.",
    remediation: "Move the `break`/`return`/`continue`/`throw` statement \
                  inside the `{ }` block of the case clause. Placing it \
                  outside creates an inconsistent style where the block looks \
                  complete but the terminator dangles after the closing brace.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "import-no-commonjs",
    description: "CommonJS `require` calls and `module.exports` are forbidden.",
    remediation: "Use ES module `import`/`export` syntax instead of `require()` and `module.exports`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/no-commonjs.md"),
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-dataset",
    description: "Prefer `.dataset` over `.setAttribute('data-*')` / `.getAttribute('data-*')`.",
    remediation: "Replace `.setAttribute('data-foo', v)` with `.dataset.foo = v` and \
                  `.getAttribute('data-foo')` with `.dataset.foo`. The `dataset` API \
                  is cleaner and avoids string-based attribute manipulation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-returns-description",
    description: "Every `@returns` tag must include a description.",
    remediation: "Add a description to the `@returns` tag explaining what the function returns.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-returns-description.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "switch-case-break-position",
    description: "`break`/`return` should be inside the case block, not after it.",
    remediation: "Move the `break`/`return`/`continue`/`throw` statement \
                  inside the `{ }` block of the case clause. Placing it \
                  outside creates an inconsistent style where the block looks \
                  complete but the terminator dangles after the closing brace.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-blob-reading-methods",
    description: "Prefer `Blob#text()` / `Blob#arrayBuffer()` over `FileReader` methods.",
    remediation: "Use `await blob.text()` instead of `reader.readAsText(blob)`, or `await blob.arrayBuffer()` instead of `reader.readAsArrayBuffer(blob)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-pub-enum-without-non-exhaustive",
    description: "`pub enum` without `#[non_exhaustive]` makes new variants a breaking change.",
    remediation: "Add `#[non_exhaustive]` above the enum. Downstream \
                  crates will need a wildcard `_ => …` arm to match it, \
                  which means future-you can add variants without \
                  releasing a major version.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-legacy-features",
    description: "Regex uses legacy RegExp static properties like `RegExp.$1` or `RegExp.lastMatch`.",
    remediation: "Avoid legacy RegExp static properties. Use capturing groups and match results instead.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-legacy-features.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-section-divider-comments",
    description: "ASCII section dividers signal a file doing too many things.",
    remediation: "Remove the divider and split the file by responsibility — each \
                  section becomes its own module. Section dividers in code are a \
                  hack around the real problem: the file should be smaller.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["comments"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-object-literal",
    description: "Use `{}` instead of `new Object()`.",
    remediation:
        "Replace `new Object()` with `{}` — object literals are cleaner and more idiomatic.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-section-divider-comments",
    description: "ASCII section dividers signal a file doing too many things.",
    remediation: "Remove the divider and split the file by responsibility — each \
                  section becomes its own module. Section dividers in code are a \
                  hack around the real problem: the file should be smaller.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["comments"],
};
--

--

--
pub const META: RuleMeta = RuleMeta {
    id: "redundant-type-aliases",
    description: "`type X = Y` where Y is a single type adds no structure — it's just renaming.",
    remediation: "Use the original type directly, or add structure (union, intersection, generics) to justify the alias.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-static-mut",
    description: "`static mut` is deprecated and unsafe by design.",
    remediation: "Replace `static mut FOO: T = ...` with a safe \
                  primitive: `OnceLock<T>`/`LazyLock<T>` for \
                  initialize-once values, `Mutex<T>`/`RwLock<T>` for \
                  shared mutable state, or `AtomicU64`/`AtomicBool`/etc \
                  for primitive counters and flags.",
    severity: Severity::Error,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-check-template-names",
    description: "JSDoc `@template` names must match type parameters used in the signature.",
    remediation: "Ensure every `@template` name corresponds to an actual type parameter, or remove the stale tag.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/check-template-names.md"),
    categories: &["jsdoc"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "no-duplicate-in-composite",
    description: "Duplicate types in a union or intersection are redundant.",
    remediation: "Remove the duplicate type from the composite. `A | A` simplifies to `A`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-duplicate-in-composite",
    description: "Duplicate types in a union or intersection are redundant.",
    remediation: "Remove the duplicate type from the composite. `A | A` simplifies to `A`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-trivially-nested-quantifier",
    description: "Two quantifiers are trivially nested and can be replaced with a single quantifier.",
    remediation: "Merge the nested quantifiers into a single equivalent quantifier.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-trivially-nested-quantifier.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-console-spaces",
    description: "Leading/trailing spaces in `console.log` arguments produce misaligned output.",
    remediation: "Remove the leading or trailing space from the string argument. Use comma-separated arguments for spacing instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "imports-first",
    description: "Import statements must appear before any other code.",
    remediation: "Move all import/require statements to the top of the file, before any non-import code (except directives like `'use strict'`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-optional-catch-binding",
    description: "Prefer omitting the `catch` binding parameter when it is unused.",
    remediation: "Remove the unused catch binding: use `catch { … }` instead of \
                  `catch (error) { … }`. Optional catch binding is supported in ES2019+.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-reduce",
    description: "`Array#reduce()` and `Array#reduceRight()` are not allowed.",
    remediation: "Use a `for` loop, `for...of`, or other array methods instead of `.reduce()` / `.reduceRight()` for better readability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-this-assignment",
    description: "Disallow assigning `this` to a variable.",
    remediation: "Use an arrow function instead of capturing `this` in a \
                  variable. Arrow functions lexically bind `this`, making \
                  the alias unnecessary and removing a common source of bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
--
pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-deprecated-props",
    description: "Deprecated TanStack Query props from v4.",
    remediation: "Migrate to v5 names: `cacheTime` → `gcTime`, \
                  `useErrorBoundary` → `throwOnError`. `onSuccess`/`onError`/\
                  `onSettled` are removed from `useQuery` — use `useEffect` \
                  instead (mutation callbacks still work).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "tanstack"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-set-x-to-y",
    description: "Function names like setStatusToClosed encode implementation, not intent.",
    remediation: "Rename to express the INTENT, not the storage operation: \
                  `setStatusToClosed` → `closeAccount`, `setRoleToAdmin` → `promoteToAdmin`. \
                  Callers should read like a story, not a database update.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["naming"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-collapsible-if",
    description: "Nested `if` can be merged with `&&`.",
    remediation: "Merge `if (a) { if (b) { ... } }` into `if (a && b) { ... }`. Unnecessary nesting wastes indent levels.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-rc-mutex",
    description: "`Rc<Mutex<T>>` pays the Mutex cost for zero benefit — Rc is !Send.",
    remediation: "Replace `Rc<Mutex<T>>` with `Rc<RefCell<T>>` (single-threaded \
                  interior mutability, no atomic ops). If you actually need \
                  cross-thread sharing, use `Arc<Mutex<T>>` instead.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-namespace",
    description: "TypeScript `namespace` is a legacy construct — use ES modules instead.",
    remediation: "Replace the `namespace` with ES module exports (`export` / `import`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "comma-or-logical-or-case",
    description: "Switch `case` uses comma or `||` instead of fall-through.",
    remediation:
        "Use separate `case` clauses with fall-through instead of comma or `||` in a single `case`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-typeof-undefined",
    description: "Compare with `undefined` directly instead of using `typeof`.",
    remediation: "Replace `typeof x === 'undefined'` with `x === undefined`. \
                  Modern JS engines handle `undefined` safely; the `typeof` \
                  guard is no longer necessary.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "consistent-date-clone",
    description: "Prefer `new Date(date)` over `new Date(date.getTime())` for cloning.",
    remediation: "Remove the unnecessary `.getTime()` / `.valueOf()` call — `new Date(date)` already clones correctly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-double-cast",
    description: "Double casts `as X as Y` hide misaligned types.",
    remediation: "Remove the double cast and fix the real misalignment. \
                  Either align the producer's type with the consumer's, \
                  or validate the value at the boundary using a type guard \
                  or Zod schema that actually checks the runtime shape.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
--
pub const META: RuleMeta = RuleMeta {
    id: "drizzle-fk-needs-index",
    description: "Foreign key without an index — FK columns need explicit indexes.",
    remediation: "Add `.index()` on every FK column. PostgreSQL does NOT auto-index FK columns — without an index, cascading deletes and JOIN lookups do sequential scans.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "drizzle"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "elseif-without-else",
    description: "`if/else if` chain without a final `else` clause.",
    remediation: "Add a final `else` block to handle all remaining cases explicitly, even if it's just a comment or unreachable assertion.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "blank-line-between-blocks",
    description: "Missing blank lines between logical blocks.",
    remediation: "Add a blank line before `return` statements (unless preceded by `}`) and between `const`/`let` declaration groups and function calls. Visual separation improves scannability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-keyword-prefix",
    description: "Do not prefix identifiers with keyword `new` or `class`.",
    remediation: "Rename the identifier to remove the keyword prefix. \
                  For example, `newUser` -> `user`, `classNames` -> `names` or `cssNames`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "too-many-break-or-continue",
    description: "Loop contains 2+ `break`/`continue` statements — consider refactoring.",
    remediation: "Extract the loop body into a function, use early returns, or restructure the logic. Multiple break/continue statements make loops hard to follow and often indicate the loop is doing too much.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-replace-all",
    description: "Prefer `String#replaceAll()` over `String#replace()` with a global regex.",
    remediation: "Replace `.replace(/pattern/g, replacement)` with `.replaceAll('pattern', replacement)`. \
                  `replaceAll()` is clearer in intent and avoids regex escaping pitfalls.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-blob-reading-methods",
    description: "Prefer `Blob#text()` / `Blob#arrayBuffer()` over `FileReader` methods.",
    remediation: "Use `await blob.text()` instead of `reader.readAsText(blob)`, or `await blob.arrayBuffer()` instead of `reader.readAsArrayBuffer(blob)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-max-nested-describe",
    description: "Deeply nested `describe` blocks reduce readability.",
    remediation: "Flatten the describe hierarchy to at most 5 levels deep.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/max-nested-describe.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "vue-no-options-api",
    description: "Use Composition API (`<script setup>`), not Options API.",
    remediation: "Replace `export default { data(), methods, computed }` with \
                  `<script setup lang=\"ts\">` using `ref()`, `computed()`, \
                  and plain functions. Options API is legacy in Vue 3.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "vue"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-collapsible-if",
    description: "Nested `if` can be merged with `&&`.",
    remediation: "Merge `if (a) { if (b) { ... } }` into `if (a && b) { ... }`. Unnecessary nesting wastes indent levels.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "error-message-is-remediation",
    description: "Error messages should describe what went wrong and what to do about it.",
    remediation: "Replace short/noun-only error messages like `\"Invalid\"` or `\"Not found\"` with actionable messages: `\"User not found — verify the ID and retry\"`. Good errors contain a verb and guide the reader toward a fix.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-pub-enum-without-non-exhaustive",
    description: "`pub enum` without `#[non_exhaustive]` makes new variants a breaking change.",
    remediation: "Add `#[non_exhaustive]` above the enum. Downstream \
                  crates will need a wildcard `_ => …` arm to match it, \
                  which means future-you can add variants without \
                  releasing a major version.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-prefer-quantifier",
    description: "Repeated identical characters or escape sequences in regex should use quantifiers.",
    remediation: "Use quantifiers: `aaa` -> `a{3}`, `\\d\\d\\d\\d` -> `\\d{4}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-shadow",
    description: "Variable shadowing makes code harder to reason about and can lead to bugs.",
    remediation: "Rename the inner variable to avoid shadowing the outer one.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-shadow"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "error-without-cause",
    description: "new Error(e.message) drops the original stack — pass { cause: e }.",
    remediation: "When wrapping a caught error, preserve the original stack and chain: \
                  `throw new Error('high-level message', { cause: original })`. \
                  Without `cause`, the debugger sees the wrapped message but loses \
                  the source location, type, and nested cause chain.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
--
pub const META: RuleMeta = RuleMeta {
    id: "elseif-without-else",
    description: "`if/else if` chain without a final `else` clause.",
    remediation: "Add a final `else` block to handle all remaining cases explicitly, even if it's just a comment or unreachable assertion.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-restricted-imports",
    description: "Some modules should not be imported due to deprecation, side effects, or project conventions.",
    remediation: "Replace the restricted import with the recommended alternative.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-restricted-imports"),
    categories: &["typescript"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "no-element-overwrite",
    description: "A collection element is written and immediately overwritten on the next line.",
    remediation: "Remove the first assignment or use a different key/index.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-check-tag-names",
    description: "JSDoc comments must only use recognized tag names.",
    remediation: "Replace the unknown tag with a standard JSDoc tag (`@param`, `@returns`, `@type`, etc.).",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/check-tag-names.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-array-key",
    description: "TanStack Query keys must be arrays, not strings.",
    remediation: "Wrap the string in brackets: `queryKey: ['todos']`. \
                  v5 requires arrays, and hierarchical invalidation \
                  (`invalidateQueries({ queryKey: ['todos'] })`) only \
                  works on array keys.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "tanstack"],
--
pub const META: RuleMeta = RuleMeta {
    id: "module-header",
    description: "Every file must start with a JSDoc module-header comment.",
    remediation: "Add a `/** */` block at the top of the file with two \
                  things: (1) What this module does, (2) How it works. \
                  A reader opening the file should know its purpose before \
                  scrolling to the first declaration.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-weak-keys",
    description: "Weak cryptographic key lengths are vulnerable to brute-force attacks.",
    remediation: "Use RSA >= 2048 bits and EC >= P-256. Prefer Ed25519 or P-384 for new keys.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-array-constructor",
    description: "Generic `Array` constructor is ambiguous — use array literal notation `[]`.",
    remediation: "Use `[]` or `Array.from()` instead. `Array<T>()` with type arguments is acceptable.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-array-constructor"),
    categories: &["typescript"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "relative-url-style",
    description: "Remove the `./` prefix from relative URLs in `new URL()`.",
    remediation: "Remove the leading `./` from the first argument of `new URL()`: \
                  use `new URL('file.js', base)` instead of `new URL('./file.js', base)`. \
                  The `./` is redundant in URL resolution.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "react-hoist-regex-outside-component",
    description: "Regex literals inside components are recompiled every render.",
    remediation: "Move the regex to a module-level `const` above the \
                  component. Regex literals inside a function body allocate \
                  a new RegExp object every call, defeating the JS engine's \
                  compiled-pattern cache.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-check-param-names",
    description: "JSDoc `@param` names must match actual function parameters.",
    remediation: "Update the `@param` tag name to match the function signature. Stale or mismatched param docs mislead callers.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-thenable",
    description: "Disallow `then` property on objects and classes.",
    remediation: "Rename the `then` method/property. Objects with a `then` \
                  method are treated as thenables by `await` and \
                  `Promise.resolve()`, causing unexpected behavior.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-spread",
    description: "Disallow unnecessary spread.",
    remediation: "Remove the redundant spread — `[...[1,2]]` is just `[1,2]` \
                  and `{...{a:1}}` is just `{a:1}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-identical-expressions",
    description: "Identical expressions on both sides of a binary operator are usually a bug.",
    remediation: "Use two different expressions, or simplify the expression.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-unsafe-impl-without-comment",
    description: "`unsafe impl` requires a `// SAFETY:` comment.",
    remediation: "Add a `// SAFETY: ...` comment immediately above the \
                  `unsafe impl` block. Spell out which invariants of \
                  the unsafe trait the type upholds — without it, the \
                  contract is unauditable.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "useless-string-operation",
    description: "String method result is ignored \u{2014} strings are immutable.",
    remediation: "Assign the result: `str = str.trim()`. String methods return a new value and never mutate in place.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-unsafe-references",
    description: "`page.evaluate()` runs in the browser — outer-scope variables are not available unless passed as the second argument.",
    remediation: "Pass captured variables as the second argument to \
                  `page.evaluate((arg) => { ... }, arg)`. Variables from \
                  the Node.js scope are not serialized into the browser \
                  context automatically — they will be `undefined` at \
                  runtime.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "no-unreadable-iife",
    description: "IIFE with parenthesized arrow function body is unreadable.",
    remediation: "Extract the inner expression from the arrow function body \
                  into a variable, or remove the unnecessary parentheses \
                  around the body.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-literal-enum-member",
    description: "Enum members should be initialized with literal values, not computed expressions.",
    remediation: "Replace the computed expression with a literal string or number value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-page-pause",
    description: "`page.pause()` is a debug-only API that halts test execution.",
    remediation: "Remove `page.pause()`. It opens the Playwright Inspector \
                  and blocks execution indefinitely — CI will hang until it \
                  times out.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "module-header",
    description: "Every file must start with a JSDoc module-header comment.",
    remediation: "Add a `/** */` block at the top of the file with two \
                  things: (1) What this module does, (2) How it works. \
                  A reader opening the file should know its purpose before \
                  scrolling to the first declaration.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-uniq-key",
    description: "Non-unique key in JSX list — `Math.random()`, `Date.now()`, or `uuid()` create new keys every render.",
    remediation: "Use a stable, unique identifier from the data (e.g., `item.id`). Random keys destroy React's reconciliation and cause performance issues.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-deprecated-props",
    description: "Deprecated TanStack Query props from v4.",
    remediation: "Migrate to v5 names: `cacheTime` → `gcTime`, \
                  `useErrorBoundary` → `throwOnError`. `onSuccess`/`onError`/\
                  `onSettled` are removed from `useQuery` — use `useEffect` \
                  instead (mutation callbacks still work).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "tanstack"],
--
pub const META: RuleMeta = RuleMeta {
    id: "jsx-no-leaked-render",
    description: "Numeric value used with `&&` in JSX renders `0` instead of nothing.",
    remediation: "Convert to boolean: `{!!count && <Component />}` or use a ternary: `{count > 0 ? <Component /> : null}`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
};

--
--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-string-refs",
    description: "String `ref` attributes are deprecated — use `useRef` / callback refs.",
    remediation: "Replace `ref=\"myRef\"` with a `useRef()` hook or a callback ref. \
                  String refs are a legacy API that has been removed in React 19.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-string-refs.md"),
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-eval",
    description: "`eval()` enables arbitrary code injection.",
    remediation: "Remove `eval()`. Use `JSON.parse()` for data, a proper parser for expressions, or a sandboxed interpreter if dynamic execution is truly needed.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-param",
    description: "JSDoc block must document every function parameter with `@param`.",
    remediation: "Add a `@param` tag for each undocumented parameter. Callers rely on JSDoc to understand the API without reading implementation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-namespace-keyword",
    description: "Use `namespace` instead of `module` to declare custom TypeScript modules.",
    remediation: "Replace the `module` keyword with `namespace`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-namespace-keyword"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-invalid-void-type",
    description: "`void` is only valid as a return type or generic type argument.",
    remediation: "Use `undefined` instead of `void` outside of return types.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-invalid-void-type/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "consistent-date-clone",
    description: "Prefer `new Date(date)` over `new Date(date.getTime())` for cloning.",
    remediation: "Remove the unnecessary `.getTime()` / `.valueOf()` call — `new Date(date)` already clones correctly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-weak-hashing",
    description: "MD5 and SHA-1 are cryptographically broken — use SHA-256 or stronger.",
    remediation: "Replace `createHash('md5')` / `createHash('sha1')` with `createHash('sha256')` or use `crypto.subtle.digest('SHA-256', …)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-unwrap-in-from-impl",
    description: "`From::from` must be infallible — no `.unwrap()` / `.expect()`.",
    remediation: "Switch the trait to `TryFrom`. Its associated `Error` \
                  type lets the caller pattern-match on the failure mode \
                  instead of panicking. `From` is reserved for total \
                  conversions; if you can write `unwrap()`, you don't \
                  have a total conversion.",
    severity: Severity::Error,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-prefer-quantifier",
    description: "Repeated identical characters or escape sequences in regex should use quantifiers.",
    remediation: "Use quantifiers: `aaa` -> `a{3}`, `\\d\\d\\d\\d` -> `\\d{4}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "custom-error-definition",
    description: "Enforce correct Error subclassing.",
    remediation: "Use a class field `name = 'MyError';` instead of setting \
                  `this.name` in the constructor. Pass the error message to \
                  `super()` instead of setting `this.message`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-after-reluctant",
    description: "Reluctant quantifier followed by end-of-pattern or group is useless.",
    remediation: "Remove the `?` from the quantifier — it has no effect when nothing follows it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-sort-flags",
    description: "Regex flags should be alphabetically sorted for consistency (`dgimsvy`).",
    remediation: "Reorder the flags alphabetically: e.g. `/pattern/gi` → `/pattern/gi` is already sorted, but `/pattern/ig` → `/pattern/gi`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "for-loop-increment-sign",
    description: "For-loop increment goes the wrong direction relative to the condition.",
    remediation: "Fix the increment direction: use `i++` with `i <` conditions and `i--` with `i >` conditions.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-slow-pattern",
    description: "Regex has nested quantifiers that can cause catastrophic backtracking (ReDoS).",
    remediation: "Refactor to avoid nested quantifiers like `(a+)+`, `(a*)*`, `(a+)*`, `(.*)*`. Use atomic groups, possessive quantifiers, or restructure the pattern.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-control-chars",
    description: "Control characters in regex are usually unintended.",
    remediation: "Remove `\\x00`-`\\x1f` control character escapes from the regex.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-replace-all",
    description: "Prefer `String#replaceAll()` over `String#replace()` with a global regex.",
    remediation: "Replace `.replace(/pattern/g, replacement)` with `.replaceAll('pattern', replacement)`. \
                  `replaceAll()` is clearer in intent and avoids regex escaping pitfalls.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "relative-url-style",
    description: "Remove the `./` prefix from relative URLs in `new URL()`.",
    remediation: "Remove the leading `./` from the first argument of `new URL()`: \
                  use `new URL('file.js', base)` instead of `new URL('./file.js', base)`. \
                  The `./` is redundant in URL resolution.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "empty-brace-spaces",
    description: "Do not add spaces between braces.",
    remediation: "Remove whitespace between empty braces: `{  }` -> `{}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-deprecated-cipher",
    description: "`createCipher()` derives the key from a password using MD5 — use `createCipheriv()` instead.",
    remediation: "Replace `crypto.createCipher(algo, password)` with `crypto.createCipheriv(algo, key, iv)`. The deprecated function uses MD5 to derive the key, which is insecure and non-standard.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "text-encoding-identifier-case",
    description: "Enforce consistent case for text encoding identifiers (`utf-8`, `ascii`).",
    remediation: "Use lowercase: `'utf-8'` instead of `'UTF-8'`, `'ascii'` instead of `'ASCII'`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-sort-flags",
    description: "Regex flags should be alphabetically sorted for consistency (`dgimsvy`).",
    remediation: "Reorder the flags alphabetically: e.g. `/pattern/gi` → `/pattern/gi` is already sorted, but `/pattern/ig` → `/pattern/gi`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-misleading-array-reverse",
    description: "`.reverse()`, `.sort()`, `.fill()`, `.splice()` mutate in place — assigning or returning the result is misleading.",
    remediation: "These methods mutate the original array and return the same reference. Use `[...arr].reverse()` or `arr.toReversed()` to avoid mutating the original.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "banned-identifiers",
    description: "Banned prefixes describe mechanics, not intent.",
    remediation: "Rename to express what this accomplishes, not how. \
                  `processOrder` → `fulfillOrder`, `handlePayment` → `chargeCustomer`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
    crate::register_ts_family_with_clippy_marker!(META, typescript, "clippy::disallowed_names")
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-object-literal",
    description: "Use `{}` instead of `new Object()`.",
    remediation:
        "Replace `new Object()` with `{}` — object literals are cleaner and more idiomatic.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-hoist-regex-outside-component",
    description: "Regex literals inside components are recompiled every render.",
    remediation: "Move the regex to a module-level `const` above the \
                  component. Regex literals inside a function body allocate \
                  a new RegExp object every call, defeating the JS engine's \
                  compiled-pattern cache.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-ignored-return",
    description: "Return value of a pure method is ignored — the call has no effect.",
    remediation: "Assign or return the result: `const result = arr.map(...)` or use a side-effect method instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-identical-expressions",
    description: "Identical expressions on both sides of a binary operator are usually a bug.",
    remediation: "Use two different expressions, or simplify the expression.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "empty-brace-spaces",
    description: "Do not add spaces between braces.",
    remediation: "Remove whitespace between empty braces: `{  }` -> `{}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "consistent-existence-index-check",
    description: "Enforce `=== -1` / `!== -1` for index existence checks.",
    remediation: "Use `index === -1` to check non-existence and `index !== -1` to check existence, instead of `< 0`, `>= 0`, or `> -1`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-duplicate-props",
    description: "Duplicate props in JSX — the last one silently wins.",
    remediation: "Remove the duplicate prop. When the same prop name appears \
                  multiple times on a JSX element, only the last value takes \
                  effect, which is almost always a copy-paste mistake.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-this-assignment",
    description: "Disallow assigning `this` to a variable.",
    remediation: "Use an arrow function instead of capturing `this` in a \
                  variable. Arrow functions lexically bind `this`, making \
                  the alias unnecessary and removing a common source of bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-same-argument-assert",
    description: "Asserting a value equals itself is always true and tests nothing.",
    remediation: "Use different expected and actual values: `expect(actual).toBe(expected)` where `actual` and `expected` are distinct.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-pascal-case",
    description: "User-defined JSX components must use PascalCase.",
    remediation: "Rename the component to PascalCase (e.g., `MyComponent` instead \
                  of `my_component` or `myComponent`).",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/jsx-pascal-case.md"),
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-put-method",
    description: "PUT replaces the entire resource; PATCH updates fields.",
    remediation: "Replace `method: 'PUT'` with `method: 'PATCH'` for \
                  partial updates. PUT requires you to send every field \
                  every time; PATCH accepts only the fields you want to \
                  change. Use PUT only when you genuinely want full \
                  replacement semantics, and comment why.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "no-same-argument-assert",
    description: "Asserting a value equals itself is always true and tests nothing.",
    remediation: "Use different expected and actual values: `expect(actual).toBe(expected)` where `actual` and `expected` are distinct.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-element-overwrite",
    description: "A collection element is written and immediately overwritten on the next line.",
    remediation: "Remove the first assignment or use a different key/index.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-stateful-global",
    description: "Global regex used with `.test()` or `.exec()` is stateful via `lastIndex`.",
    remediation: "Remove the `g` flag if using `.test()` or `.exec()` repeatedly, or create the regex inside the loop. The `g` flag makes `lastIndex` persist across calls, causing alternating true/false results.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-sql-string-format",
    description: "SQL queries built with template literals or string concatenation are vulnerable to injection.",
    remediation: "Use parameterized queries or prepared statements instead of interpolating values into SQL strings.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-equality-matcher",
    description: "Use an equality matcher instead of `expect(a === b).toBe(true)`.",
    remediation: "Replace with `expect(a).toBe(b)` or `expect(a).toEqual(b)`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-equality-matcher.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-invalid-html-attribute",
    description: "Invalid value in HTML `rel` attribute.",
    remediation: "Use a valid `rel` value. Common valid values for `<a>` include \
                  `noopener`, `noreferrer`, `nofollow`. For `<link>` they include \
                  `stylesheet`, `icon`, `preload`, `prefetch`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-invalid-html-attribute.md"),
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-sql-string-format",
    description: "SQL queries built with template literals or string concatenation are vulnerable to injection.",
    remediation: "Use parameterized queries or prepared statements instead of interpolating values into SQL strings.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-literal-enum-member",
    description: "Enum members should be initialized with literal values, not computed expressions.",
    remediation: "Replace the computed expression with a literal string or number value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-typeof-undefined",
    description: "Compare with `undefined` directly instead of using `typeof`.",
    remediation: "Replace `typeof x === 'undefined'` with `x === undefined`. \
                  Modern JS engines handle `undefined` safely; the `typeof` \
                  guard is no longer necessary.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "useless-string-operation",
    description: "String method result is ignored \u{2014} strings are immutable.",
    remediation: "Assign the result: `str = str.trim()`. String methods return a new value and never mutate in place.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-string-match",
    description: "Regex that matches the empty string used in `.split()` or `.replace()`.",
    remediation: "A pattern like `*`, `?`, or `{0,}` can match zero characters, causing unexpected splits or replacements. Use `+` or `{1,}` instead, or add anchors.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-namespace",
    description: "Namespaced JSX elements (`<Foo:bar>`) are not supported by React.",
    remediation: "React does not support XML namespaces in JSX. Use a different \
                  naming pattern (e.g., `FooBar` or `Foo.Bar`).",
    severity: Severity::Error,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-namespace.md"),
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "drizzle-fk-needs-index",
    description: "Foreign key without an index — FK columns need explicit indexes.",
    remediation: "Add `.index()` on every FK column. PostgreSQL does NOT auto-index FK columns — without an index, cascading deletes and JOIN lookups do sequential scans.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "drizzle"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-ignored-return",
    description: "Return value of a pure method is ignored — the call has no effect.",
    remediation: "Assign or return the result: `const result = arr.map(...)` or use a side-effect method instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-intersection",
    description: "Intersecting with `any` or `unknown` is useless — `& any` produces `any`, `& unknown` is a no-op.",
    remediation: "Remove the `& any` or `& unknown` from the intersection.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsx-no-leaked-render",
    description: "Numeric value used with `&&` in JSX renders `0` instead of nothing.",
    remediation: "Convert to boolean: `{!!count && <Component />}` or use a ternary: `{count > 0 ? <Component /> : null}`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-restricted-types",
    description: "Certain types are banned by project convention or because better alternatives exist.",
    remediation: "Replace the restricted type with the recommended alternative.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-restricted-types"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-weak-hashing",
    description: "MD5 and SHA-1 are cryptographically broken — use SHA-256 or stronger.",
    remediation: "Replace `createHash('md5')` / `createHash('sha1')` with `createHash('sha256')` or use `crypto.subtle.digest('SHA-256', …)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-unsafe-references",
    description: "`page.evaluate()` runs in the browser — outer-scope variables are not available unless passed as the second argument.",
    remediation: "Pass captured variables as the second argument to \
                  `page.evaluate((arg) => { ... }, arg)`. Variables from \
                  the Node.js scope are not serialized into the browser \
                  context automatically — they will be `undefined` at \
                  runtime.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "no-weak-ssl",
    description: "Weak SSL/TLS protocol versions are insecure.",
    remediation: "Use TLSv1.2 or TLSv1.3. Older protocols (SSLv2, SSLv3, TLSv1.0, TLSv1.1) have known vulnerabilities.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "custom-error-definition",
    description: "Enforce correct Error subclassing.",
    remediation: "Use a class field `name = 'MyError';` instead of setting \
                  `this.name` in the constructor. Pass the error message to \
                  `super()` instead of setting `this.message`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-invalid-remove-event-listener",
    description: "`removeEventListener` with an inline function or `.bind()` call never matches the original listener.",
    remediation: "Pass a stable function reference to `removeEventListener` — store the bound/arrow function in a variable first.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-nested-template-literal",
    description: "Nested template literal — extract to a named variable.",
    remediation: "Extract the inner template to a named variable. Nested backticks are hard to read and easy to misparse.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-misleading-array-reverse",
    description: "`.reverse()`, `.sort()`, `.fill()`, `.splice()` mutate in place — assigning or returning the result is misleading.",
    remediation: "These methods mutate the original array and return the same reference. Use `[...arr].reverse()` or `arr.toReversed()` to avoid mutating the original.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-uniq-key",
    description: "Non-unique key in JSX list — `Math.random()`, `Date.now()`, or `uuid()` create new keys every render.",
    remediation: "Use a stable, unique identifier from the data (e.g., `item.id`). Random keys destroy React's reconciliation and cause performance issues.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "explicit-units",
    description: "Numeric names should include an explicit unit (Ms, Bytes, Kb...).",
    remediation: "Add a unit suffix: `delay` → `delayMs`, `size` → \
                  `sizeBytes`, `rate` → `rateRps`. Ambiguous units cause \
                  real bugs — setTimeout(delay) expects ms.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-use-before-define",
    description: "Using variables before their definition leads to confusing code and potential TDZ errors.",
    remediation: "Move the declaration before its first usage, or restructure the code.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-use-before-define"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "auth-on-mutation",
    description: "Mutation route handlers (POST/PUT/DELETE/PATCH) should reference auth.",
    remediation: "Add an auth check (`auth`, `token`, `session`, `middleware`, `guard`, `protect`, or `verify`) to mutation route handlers. Missing auth on mutations is a common security gap.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-eval",
    description: "`eval()` enables arbitrary code injection.",
    remediation: "Remove `eval()`. Use `JSON.parse()` for data, a proper parser for expressions, or a sandboxed interpreter if dynamic execution is truly needed.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-all-duplicated-branches",
    description: "All branches have the same implementation — the conditional is pointless.",
    remediation: "Remove the conditional and keep just the body. Duplicated branches hide that the branching is no longer meaningful.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-logical-operator-over-ternary",
    description: "Prefer `||`/`??` over a ternary that repeats the test in a branch.",
    remediation: "Replace `foo ? foo : bar` with `foo || bar` (or `foo ?? bar`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-control-chars",
    description: "Control characters in regex are usually unintended.",
    remediation: "Remove `\\x00`-`\\x1f` control character escapes from the regex.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "vue-no-options-api",
    description: "Use Composition API (`<script setup>`), not Options API.",
    remediation: "Replace `export default { data(), methods, computed }` with \
                  `<script setup lang=\"ts\">` using `ref()`, `computed()`, \
                  and plain functions. Options API is legacy in Vue 3.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "vue"],
};
--

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-unwrap-in-from-impl",
    description: "`From::from` must be infallible — no `.unwrap()` / `.expect()`.",
    remediation: "Switch the trait to `TryFrom`. Its associated `Error` \
                  type lets the caller pattern-match on the failure mode \
                  instead of panicking. `From` is reserved for total \
                  conversions; if you can write `unwrap()`, you don't \
                  have a total conversion.",
    severity: Severity::Error,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-file-overview",
    description: "Files must contain a `@file` or `@fileoverview` JSDoc tag.",
    remediation: "Add a `/** @file ... */` or `/** @fileoverview ... */` comment at the top of the file.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-file-overview.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "drizzle-timestamp-with-timezone",
    description: "`timestamp('col')` is timezone-ambiguous.",
    remediation: "Add `{ withTimezone: true }` to every timestamp column. \
                  Bare timestamps are interpreted differently depending \
                  on the server's zone, silently corrupting dates.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "drizzle"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "consistent-template-literal-escape",
    description: "Use `\\${` instead of `$\\{` to escape in template literals.",
    remediation: "Escape the dollar sign (`\\${`) rather than the opening brace (`$\\{`) or both (`\\$\\{`). This is the consistent way to prevent expression interpolation in template literals.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-intersection",
    description: "Intersecting with `any` or `unknown` is useless — `& any` produces `any`, `& unknown` is a no-op.",
    remediation: "Remove the `& any` or `& unknown` from the intersection.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-nested-incdec",
    description: "`++` or `--` used inside an expression, not as a standalone statement.",
    remediation: "Separate the increment/decrement from the expression. Write `i++; arr[i] = x;` instead of `arr[i++] = x;` to make the order of operations explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-manual-rtl-cleanup",
    description: "Importing `cleanup` from `@testing-library/react` in Vitest causes double cleanup.",
    remediation: "Remove the `cleanup` import and any `afterEach(cleanup)` \
                  call. Vitest with `@testing-library/react` runs cleanup \
                  automatically after each test. Manual cleanup causes \
                  double cleanup which can mask unmount bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-println-in-library",
    description: "Library code must use tracing, not `println!` / `eprintln!`.",
    remediation: "Replace `println!` with `tracing::info!` / `tracing::debug!` \
                  and add structured fields. Library consumers configure the \
                  tracing subscriber; they cannot redirect `println!`. The rule \
                  auto-skips pure binary crates where writing to stdout is the \
                  whole point.",
    severity: Severity::Error,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-method-signature-style",
    description: "Shorthand method signatures in interfaces are less safe than property signatures — they allow unsafe variance.",
    remediation: "Use a property signature with a function type: `foo: (x: string) => void` instead of `foo(x: string): void`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/method-signature-style"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-string-literal",
    description: "String disjunction of single characters in a `v`-flag character class can be simplified.",
    remediation: "Replace the string disjunction with a simple character class element.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-useless-string-literal.html"),
    categories: &["regex"],
};

--
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-undocumented-unsafe",
    description: "Every `unsafe` block must have a `// SAFETY:` comment.",
    remediation: "Add a `// SAFETY: ...` comment above every `unsafe { ... }` \
                  block explaining the invariants that make the unsafe code \
                  sound. The comment is what future debuggers will reach for \
                  when memory corruption shows up.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-find",
    description: "Prefer `.find(…)` over `.filter(…)[0]` or `.filter(…).at(0)`.",
    remediation: "Replace `.filter(…)[0]` with `.find(…)` to short-circuit on the first match.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-property-name",
    description: "Every `@property` tag must include a property name.",
    remediation: "Add the property name after the type annotation — `@property {string} name` instead of `@property {string}`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-property-name.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "for-loop-increment-sign",
    description: "For-loop increment goes the wrong direction relative to the condition.",
    remediation: "Fix the increment direction: use `i++` with `i <` conditions and `i--` with `i >` conditions.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-comment-textnodes",
    description: "Comments placed as JSX text children are rendered as literal text.",
    remediation: "Use `{/* comment */}` for JSX comments, not `// comment` or \
                  `/* comment */` as bare text. Without braces, the comment \
                  syntax is rendered as visible text in the DOM.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-empty-file",
    description: "Empty files are not allowed — they add noise without value.",
    remediation: "Add meaningful content or delete the file.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-enum",
    description: "TypeScript enums emit runtime code and don't narrow cleanly.",
    remediation: "Replace `enum` with `const X = { ... } as const satisfies \
                  Record<string, string>` for config, or a discriminated \
                  union with a `type`/`kind` field for tagged data.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-linkedlist",
    description: "Prefer `Vec<T>` over `LinkedList<T>` — cache locality wins.",
    remediation: "Replace `LinkedList<T>` with `Vec<T>` or `VecDeque<T>`. \
                  LinkedList's theoretical O(1) splice is dominated in \
                  practice by Vec's cache locality for any realistic size. \
                  Enable `clippy::linkedlist`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-unnecessary-slice-end",
    description: "Disallow unnecessary `.length` or `Infinity` as the `end` argument of `slice()`.",
    remediation: "Remove the second argument: `.slice(start)` already goes to the end.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-nested-incdec",
    description: "`++` or `--` used inside an expression, not as a standalone statement.",
    remediation: "Separate the increment/decrement from the expression. Write `i++; arr[i] = x;` instead of `arr[i++] = x;` to make the order of operations explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-wait-for-navigation",
    description: "`page.waitForNavigation()` is discouraged — use `waitForURL` instead.",
    remediation: "Replace `waitForNavigation()` with `page.waitForURL(url)` \
                  or a web-first assertion.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-wait-for-navigation.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-useless-await",
    description: "Unnecessary `await` on synchronous Playwright methods.",
    remediation: "Remove the `await` — this method does not return a Promise.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-useless-await.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-deprecated-cipher",
    description: "`createCipher()` derives the key from a password using MD5 — use `createCipheriv()` instead.",
    remediation: "Replace `crypto.createCipher(algo, password)` with `crypto.createCipheriv(algo, key, iv)`. The deprecated function uses MD5 to derive the key, which is insecure and non-standard.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "operation-returning-nan",
    description: "Arithmetic operation will produce `NaN`.",
    remediation: "Convert the operand to a number first (`Number(x)`, `parseInt(x)`, `+x`) or fix the expression. Arithmetic on `undefined` or non-numeric strings always returns `NaN`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-slow-pattern",
    description: "Regex has nested quantifiers that can cause catastrophic backtracking (ReDoS).",
    remediation: "Refactor to avoid nested quantifiers like `(a+)+`, `(a*)*`, `(a+)*`, `(.*)*`. Use atomic groups, possessive quantifiers, or restructure the pattern.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "boolean-naming",
    description: "Boolean identifiers must start with is/has/should/can/will/did/was.",
    remediation: "Rename to convey the predicate: `ready` → `isReady` (TS) or \
                  `is_ready` (Rust). Use the positive form only — prefer \
                  `!isReady` over `isNotReady`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-index-of",
    description: "Prefer `.indexOf(val)` over `.findIndex(x => x === val)`.",
    remediation: "Replace `.findIndex(x => x === val)` with `.indexOf(val)` for simple equality checks.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "banned-identifiers",
    description: "Banned prefixes describe mechanics, not intent.",
    remediation: "Rename to express what this accomplishes, not how. \
                  `processOrder` → `fulfillOrder`, `handlePayment` → `chargeCustomer`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-incomplete-assertions",
    description: "Assertion chain is missing the actual matcher.",
    remediation: "Complete the assertion with a matcher: `expect(x).toBe(...)`, `.toEqual(...)`, `.toThrow()`, etc. Bare `expect(x);` or `expect(x).not;` tests nothing.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "text-encoding-identifier-case",
    description: "Enforce consistent case for text encoding identifiers (`utf-8`, `ascii`).",
    remediation: "Use lowercase: `'utf-8'` instead of `'UTF-8'`, `'ascii'` instead of `'ASCII'`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-undocumented-unsafe",
    description: "Every `unsafe` block must have a `// SAFETY:` comment.",
    remediation: "Add a `// SAFETY: ...` comment above every `unsafe { ... }` \
                  block explaining the invariants that make the unsafe code \
                  sound. The comment is what future debuggers will reach for \
                  when memory corruption shows up.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-collection-argument",
    description: "Disallow useless values in `Set`, `Map`, `WeakSet`, or `WeakMap` constructors.",
    remediation: "Remove the empty/null/undefined argument from the collection \
                  constructor. `new Set([])` and `new Map(undefined)` are \
                  equivalent to `new Set()` and `new Map()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-duplicate-props",
    description: "Duplicate props in JSX — the last one silently wins.",
    remediation: "Remove the duplicate prop. When the same prop name appears \
                  multiple times on a JSX element, only the last value takes \
                  effect, which is almost always a copy-paste mistake.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-class-methods-use-this",
    description: "Class methods that don't use `this` should be static or extracted to a standalone function.",
    remediation: "Add `static` to the method, move it to a standalone function, or use `this` in the body.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/class-methods-use-this"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-classlist-toggle",
    description: "Prefer `Element#classList.toggle()` over conditional `add`/`remove`.",
    remediation: "Replace `if (c) el.classList.add('x') else el.classList.remove('x')` with `el.classList.toggle('x', c)`. The `toggle` method with a force argument is cleaner and avoids conditional branching.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-linkedlist",
    description: "Prefer `Vec<T>` over `LinkedList<T>` — cache locality wins.",
    remediation: "Replace `LinkedList<T>` with `Vec<T>` or `VecDeque<T>`. \
                  LinkedList's theoretical O(1) splice is dominated in \
                  practice by Vec's cache locality for any realistic size. \
                  Enable `clippy::linkedlist`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "consistent-existence-index-check",
    description: "Enforce `=== -1` / `!== -1` for index existence checks.",
    remediation: "Use `index === -1` to check non-existence and `index !== -1` to check existence, instead of `< 0`, `>= 0`, or `> -1`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "no-nested-ternary",
    description: "Nested ternaries are hard to read and easy to misparse.",
    remediation: "Nested ternary — extract to if/else or a named variable for each branch.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
    crate::register_ts_family!(META, typescript)
}
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-stateful-global",
    description: "Global regex used with `.test()` or `.exec()` is stateful via `lastIndex`.",
    remediation: "Remove the `g` flag if using `.test()` or `.exec()` repeatedly, or create the regex inside the loop. The `g` flag makes `lastIndex` persist across calls, causing alternating true/false results.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "node-no-path-concat",
    description: "String concatenation with `__dirname` / `__filename` is platform-dependent.",
    remediation: "Use `path.join()` or `path.resolve()` instead of string concatenation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

--
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-loop-func",
    description: "Functions declared inside loops often cause bugs due to closures capturing the loop variable by reference.",
    remediation: "Move the function outside the loop, or use `let`/`const` in a `for` loop to create a new binding per iteration.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-loop-func"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-date-now",
    description: "Prefer `Date.now()` over `new Date().getTime()`, `+new Date()`, or `Number(new Date())`.",
    remediation: "Replace with `Date.now()`. It is clearer, avoids allocating a throwaway `Date` object, and is faster.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "throw-new-error",
    description: "Use `new` when creating an error.",
    remediation: "Replace `throw Error(...)` with `throw new Error(...)`. \
                  Calling Error without `new` is valid but inconsistent and \
                  can confuse readers about whether a new instance is created.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-constructor-side-effects",
    description: "`new X()` without assignment is a side-effect anti-pattern.",
    remediation: "Assign the result of `new X()` to a variable, or refactor side effects out of the constructor into a static method.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-string-match",
    description: "Regex that matches the empty string used in `.split()` or `.replace()`.",
    remediation: "A pattern like `*`, `?`, or `{0,}` can match zero characters, causing unexpected splits or replacements. Use `+` or `{1,}` instead, or add anchors.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "consistent-empty-array-spread",
    description: "Parenthesize ternaries spread into array literals.",
    remediation: "Wrap the ternary in parentheses: `[...(condition ? ['a'] : [])]` \
                  instead of `[...condition ? ['a'] : []]`. Without parens the \
                  precedence is ambiguous and confusing.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-confusing-non-null-assertion",
    description: "`a! == b` looks confusingly like `a !== b`.",
    remediation: "Remove the `!` or wrap the left side in parentheses: `(a!) == b`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "node-no-path-concat",
    description: "String concatenation with `__dirname` / `__filename` is platform-dependent.",
    remediation: "Use `path.join()` or `path.resolve()` instead of string concatenation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-disable-mustache-escape",
    description: "Disabling template engine HTML escaping (`escapeMarkup = false`) opens XSS vectors.",
    remediation: "Keep HTML escaping enabled. If raw HTML is truly needed, sanitize it explicitly before rendering.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-println-in-library",
    description: "Library code must use tracing, not `println!` / `eprintln!`.",
    remediation: "Replace `println!` with `tracing::info!` / `tracing::debug!` \
                  and add structured fields. Library consumers configure the \
                  tracing subscriber; they cannot redirect `println!`. The rule \
                  auto-skips pure binary crates where writing to stdout is the \
                  whole point.",
    severity: Severity::Error,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-default-last",
    description: "`default` clause in switch should be the last clause.",
    remediation: "Move the `default:` clause to the end of the switch statement for readability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "hono-secure-headers-disabled",
    description: "Security header explicitly disabled in `secureHeaders()`.",
    remediation: "Don't disable security headers. Each one protects against a specific attack vector (HSTS, clickjacking, MIME sniffing, fingerprinting).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-for-loop",
    description: "Use a `for-of` loop instead of this `for` loop.",
    remediation: "Replace `for (let i = 0; i < arr.length; i++)` with \
                  `for (const item of arr)`. If the index is needed, use \
                  `for (const [i, item] of arr.entries())`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-max-expects",
    description: "Too many assertions in a single test — split into focused tests.",
    remediation: "Keep each test to ≤ 5 `expect()` calls. Extract additional \
                  assertions into separate test cases.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/max-expects.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-unit-error-result",
    description: "`Result<T, ()>` discards every error detail.",
    remediation: "Define a real error type — even a tiny enum — and use \
                  it as the `E` parameter. If absence is the only failure \
                  mode, return `Option<T>` instead. `Result<T, ()>` is the \
                  worst of both worlds.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-verb-in-rest-url",
    description: "REST URLs should identify resources, not actions.",
    remediation: "Replace verb-in-URL patterns with HTTP semantics: \
                  `POST /api/orders` to create, `GET /api/orders/:id` to \
                  read, `PATCH /api/orders/:id` to update, \
                  `DELETE /api/orders/:id` to remove.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
--
pub const META: RuleMeta = RuleMeta {
    id: "redundant-type-aliases",
    description: "`type X = Y` where Y is a single type adds no structure — it's just renaming.",
    remediation: "Use the original type directly, or add structure (union, intersection, generics) to justify the alias.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unused-private-class-members",
    description: "Private class members that are never used are dead code.",
    remediation: "Remove the unused private member or use it.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-unused-private-class-members"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-optimal-lookaround-quantifier",
    description: "Quantified expression at the edge of a lookaround should only match a constant number of times.",
    remediation: "Remove or simplify the quantifier at the start/end of the lookaround expression.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/optimal-lookaround-quantifier.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unnecessary-slice-end",
    description: "Disallow unnecessary `.length` or `Infinity` as the `end` argument of `slice()`.",
    remediation: "Remove the second argument: `.slice(start)` already goes to the end.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-hidden-control-flow",
    description: "3+ decorators stacked on a single function/class hide control flow.",
    remediation: "Reduce the decorator stack to 2 or fewer. Each decorator adds invisible control flow — stacking 3+ makes the execution path hard to reason about. Compose decorators into a single higher-level one or use explicit middleware.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "consistent-empty-array-spread",
    description: "Parenthesize ternaries spread into array literals.",
    remediation: "Wrap the ternary in parentheses: `[...(condition ? ['a'] : [])]` instead of `[...condition ? ['a'] : []]`. Without parens the precedence is ambiguous and confusing.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unnecessary-array-flat-depth",
    description: "Disallow using `1` as the `depth` argument of `Array#flat()`.",
    remediation: "Remove the argument: `.flat()` defaults to depth 1.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-timestamp-without-tz",
    description: "`TIMESTAMP` without timezone — use `TIMESTAMPTZ` to avoid timezone bugs.",
    remediation: "Replace `TIMESTAMP` with `TIMESTAMPTZ` (or `TIMESTAMP WITH TIME ZONE`). Without timezone info, the same instant is interpreted differently depending on the server's `timezone` setting.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-duplicate-classes",
    description: "Duplicate CSS classes in className/class attributes are redundant and confusing.",
    remediation: "Remove the duplicate class. Each utility should appear at most once.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-default-last",
    description: "`default` clause in switch should be the last clause.",
    remediation: "Move the `default:` clause to the end of the switch statement for readability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "operation-returning-nan",
    description: "Arithmetic operation will produce `NaN`.",
    remediation: "Convert the operand to a number first (`Number(x)`, `parseInt(x)`, `+x`) or fix the expression. Arithmetic on `undefined` or non-numeric strings always returns `NaN`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "generator-without-yield",
    description: "Generator function does not contain a `yield` expression.",
    remediation: "Add a `yield` expression or convert to a regular function. A generator without `yield` is misleading — callers expect lazy iteration.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-immediate-mutation",
    description: "Disallow immediate mutation after variable assignment.",
    remediation: "Chain the mutation onto the initialiser: \
                  `const arr = [3,1,2].sort()` instead of declaring then \
                  mutating on the next line. This makes the intent clearer \
                  and avoids an intermediate mutable state.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-reduce",
    description: "`Array#reduce()` and `Array#reduceRight()` are not allowed.",
    remediation: "Use a `for` loop, `for...of`, or other array methods instead of `.reduce()` / `.reduceRight()` for better readability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-promise-reject",
    description: "`Promise.reject()` makes error handling harder — prefer returning error values or throwing typed errors.",
    remediation: "Return a Result type, throw a typed error, or use `Promise.resolve()` with an error discriminant.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "no-gratuitous-expression",
    description: "Boolean expression is always true or always false.",
    remediation: "Remove the dead branch. A condition that can never flip is either a bug or leftover from a refactor.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-unit-error-result",
    description: "`Result<T, ()>` discards every error detail.",
    remediation: "Define a real error type — even a tiny enum — and use \
                  it as the `E` parameter. If absence is the only failure \
                  mode, return `Option<T>` instead. `Result<T, ()>` is the \
                  worst of both worlds.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-native-locators",
    description: "`locator('[role=\"button\"]')` should be `getByRole('button')` — use Playwright's built-in locators.",
    remediation: "Replace attribute-selector locators with Playwright's \
                  built-in locator methods: `[role=...]` → `getByRole()`, \
                  `[placeholder=...]` → `getByPlaceholder()`, \
                  `[alt=...]` → `getByAltText()`, \
                  `[title=...]` → `getByTitle()`, \
                  `[data-testid=...]` → `getByTestId()`. \
                  Built-in locators are more readable and provide better \
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-function-type",
    description: "An interface with only a call signature should be a function type.",
    remediation: "Replace `interface Fn { (): T }` with `type Fn = () => T`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-function-type/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-namespace",
    description: "TypeScript `namespace` is a legacy construct — use ES modules instead.",
    remediation: "Replace the `namespace` with ES module exports (`export` / `import`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-hidden-control-flow",
    description: "3+ decorators stacked on a single function/class hide control flow.",
    remediation: "Reduce the decorator stack to 2 or fewer. Each decorator adds invisible control flow — stacking 3+ makes the execution path hard to reason about. Compose decorators into a single higher-level one or use explicit middleware.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-weak-ssl",
    description: "Weak SSL/TLS protocol versions are insecure.",
    remediation: "Use TLSv1.2 or TLSv1.3. Older protocols (SSLv2, SSLv3, TLSv1.0, TLSv1.1) have known vulnerabilities.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-hooks-on-top",
    description: "Hooks should come before any test cases.",
    remediation: "Move hooks above the first `test()` / `it()` call.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-hooks-on-top.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-object-in-dep-array",
    description: "Hook dependency arrays must contain primitives, not objects/arrays.",
    remediation: "Extract the primitive field you depend on: \
                  `useEffect(() => { ... }, [user.id])` instead of `[user]`. \
                  Objects change reference on every render even when their \
                  content is identical, causing infinite re-runs.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-structured-clone",
    description:
        "Prefer `structuredClone(…)` over `JSON.parse(JSON.stringify(…))` for deep cloning.",
    remediation: "Replace `JSON.parse(JSON.stringify(x))` with `structuredClone(x)`. \
                  `structuredClone` handles circular references, typed arrays, and \
                  other values that JSON serialization silently drops or corrupts.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-use-unicode-flag",
    description: "Unicode property escapes (`\\p{...}` / `\\P{...}`) require the `u` or `v` flag.",
    remediation: "Add the `u` flag to the regex: `/\\p{Letter}/u`. Without it, `\\p` is not interpreted as a Unicode property escape.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "explicit-units",
    description: "Numeric names should include an explicit unit (Ms, Bytes, Kb...).",
    remediation: "Add a unit suffix: `delay` → `delayMs`, `size` → \
                  `sizeBytes`, `rate` → `rateRps`. Ambiguous units cause \
                  real bugs — setTimeout(delay) expects ms.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-json-parse-buffer",
    description: "Prefer reading a JSON file as a buffer.",
    remediation: "Remove the `'utf-8'` / `'utf8'` encoding argument from \
                  `fs.readFileSync()` when the result is passed to `JSON.parse()`. \
                  `JSON.parse()` accepts a `Buffer` directly, which avoids an \
                  intermediate string allocation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "justify-inaction",
    description: "Empty `catch {}`, `else {}`, or early `return;` without an explaining comment.",
    remediation: "Add a comment on the preceding line explaining why the block is intentionally empty or why the early return is correct. Silent inaction hides bugs — make the intent explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-check-values",
    description: "JSDoc `@version` and `@since` tags must contain valid semver values.",
    remediation: "Provide a valid semver string in your `@version` / `@since` tag (e.g. `1.0.0`).",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/check-values.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unused-collection",
    description: "Collection is populated but never read.",
    remediation: "Either use the collection (iterate, return, pass to a function) or remove the dead code. Populated-but-unread collections indicate logic that was never finished or already removed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-duplicate-classes",
    description: "Duplicate CSS classes in className/class attributes are redundant and confusing.",
    remediation: "Remove the duplicate class. Each utility should appear at most once.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-native-locators",
    description: "`locator('[role=\"button\"]')` should be `getByRole('button')` — use Playwright's built-in locators.",
    remediation: "Replace attribute-selector locators with Playwright's \
                  built-in locator methods: `[role=...]` → `getByRole()`, \
                  `[placeholder=...]` → `getByPlaceholder()`, \
                  `[alt=...]` → `getByAltText()`, \
                  `[title=...]` → `getByTitle()`, \
                  `[data-testid=...]` → `getByTestId()`. \
                  Built-in locators are more readable and provide better \
--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-delete",
    description: "`delete` on an array element creates a sparse hole instead of removing.",
    remediation: "Use `Array.prototype.splice()` to remove elements: `arr.splice(index, 1)` instead of `delete arr[index]`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-logical-operator-over-ternary",
    description: "Prefer `||`/`??` over a ternary that repeats the test in a branch.",
    remediation: "Replace `foo ? foo : bar` with `foo || bar` (or `foo ?? bar`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unsafe-declaration-merging",
    description: "Unsafe declaration merging between classes and interfaces.",
    remediation: "Rename one of the declarations so they don't merge.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-object-in-dep-array",
    description: "Hook dependency arrays must contain primitives, not objects/arrays.",
    remediation: "Extract the primitive field you depend on: \
                  `useEffect(() => { ... }, [user.id])` instead of `[user]`. \
                  Objects change reference on every render even when their \
                  content is identical, causing infinite re-runs.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-structured-clone",
    description:
        "Prefer `structuredClone(…)` over `JSON.parse(JSON.stringify(…))` for deep cloning.",
    remediation: "Replace `JSON.parse(JSON.stringify(x))` with `structuredClone(x)`. \
                  `structuredClone` handles circular references, typed arrays, and \
                  other values that JSON serialization silently drops or corrupts.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-equals-in-for-termination",
    description: "`for` loop uses `==` or `===` in the termination condition.",
    remediation: "Use `<`, `<=`, `>`, or `>=` instead. Equality checks in `for` termination either never execute the body or loop forever if the counter skips the target value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-arguments-usage",
    description: "Direct use of the `arguments` object is discouraged.",
    remediation: "Use rest parameters (`...args`) instead of `arguments`. Rest parameters are a real Array and work with arrow functions.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-adjacent-inline-elements",
    description: "Adjacent inline elements without whitespace between them.",
    remediation: "Add a space, `{' '}`, or a wrapper between adjacent inline \
                  elements to ensure they render with visible separation.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-adjacent-inline-elements.md"),
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-inline-param-type",
    description: "Inline object types in parameters resist reuse and refactoring.",
    remediation: "Extract the inline type to a named `type` declaration \
                  above the function. A named type has an identity, can be \
                  shared across call sites, and shows up in IDE hover.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-equals-in-for-termination",
    description: "`for` loop uses `==` or `===` in the termination condition.",
    remediation: "Use `<`, `<=`, `>`, or `>=` instead. Equality checks in `for` termination either never execute the body or loop forever if the counter skips the target value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-hooks-in-order",
    description: "Hooks should follow the lifecycle order: beforeAll, beforeEach, afterEach, afterAll.",
    remediation: "Reorder hooks to: `beforeAll` > `beforeEach` > `afterEach` > `afterAll`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-hooks-in-order.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-delete",
    description: "`delete` on an array element creates a sparse hole instead of removing.",
    remediation: "Use `Array.prototype.splice()` to remove elements: `arr.splice(index, 1)` instead of `delete arr[index]`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-missing-await",
    description: "Playwright async method call is missing `await`.",
    remediation: "Add `await` before the Playwright call. Without it the operation runs detached, causing flaky tests and race conditions.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "import-exports-last",
    description: "Export statements should appear at the end of the file.",
    remediation: "Move all `export` declarations to the bottom of the file, after all other statements.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/exports-last.md"),
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-nested-template-literal",
    description: "Nested template literal — extract to a named variable.",
    remediation: "Extract the inner template to a named variable. Nested backticks are hard to read and easy to misparse.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-constructor",
    description: "`new Array()` is ambiguous — single numeric arg creates sparse array.",
    remediation: "Use array literals `[]` or `Array.from()` instead of `new Array(...)`. `new Array(3)` creates a sparse array of length 3, not `[3]`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "consistent-template-literal-escape",
    description: "Use `\\${` instead of `$\\{` to escape in template literals.",
    remediation: "Escape the dollar sign (`\\${`) rather than the opening brace (`$\\{`) or both (`\\$\\{`). This is the consistent way to prevent expression interpolation in template literals.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-missing-await",
    description: "Playwright async method call is missing `await`.",
    remediation: "Add `await` before the Playwright call. Without it the operation runs detached, causing flaky tests and race conditions.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-arguments-usage",
    description: "Direct use of the `arguments` object is discouraged.",
    remediation: "Use rest parameters (`...args`) instead of `arguments`. Rest parameters are a real Array and work with arrow functions.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-commented-out-code",
    description: "Commented-out code is unreviewable, unreachable, and rots.",
    remediation: "Delete the commented-out code. Git history preserves the \
                  original if you need to recover it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "generator-without-yield",
    description: "Generator function does not contain a `yield` expression.",
    remediation: "Add a `yield` expression or convert to a regular function. A generator without `yield` is misleading — callers expect lazy iteration.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-top-level-format",
    description: "Zod v4 top-level format helpers are shorter and faster.",
    remediation: "Replace `z.string().email()` with `z.email()`, \
                  `z.string().url()` with `z.url()`, `z.number().int()` with \
                  `z.int()`, and similar chains. Top-level helpers are \
                  tree-shakeable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "zod"],
--
pub const META: RuleMeta = RuleMeta {
    id: "auth-on-mutation",
    description: "Mutation route handlers (POST/PUT/DELETE/PATCH) should reference auth.",
    remediation: "Add an auth check (`auth`, `token`, `session`, `middleware`, `guard`, `protect`, or `verify`) to mutation route handlers. Missing auth on mutations is a common security gap.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-empty-file",
    description: "Empty files are not allowed — they add noise without value.",
    remediation: "Add meaningful content or delete the file.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-constructor",
    description: "`new Array()` is ambiguous — single numeric arg creates sparse array.",
    remediation: "Use array literals `[]` or `Array.from()` instead of `new Array(...)`. `new Array(3)` creates a sparse array of length 3, not `[3]`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "exports-at-top",
    description: "Public API (exports) should appear before private helpers.",
    remediation: "Move all exported (`export` / `pub`) items to the top of \
                  the file. Readers should see the module's public surface \
                  at a glance without scanning through private helpers first.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-commented-out-code",
    description: "Commented-out code is unreviewable, unreachable, and rots.",
    remediation: "Delete the commented-out code. Git history preserves the \
                  original if you need to recover it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-set-operand",
    description: "Character class set operation has a useless operand that does not affect the result.",
    remediation: "Remove the useless operand from the set operation.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-useless-set-operand.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-comment-textnodes",
    description: "Comments placed as JSX text children are rendered as literal text.",
    remediation: "Use `{/* comment */}` for JSX comments, not `// comment` or \
                  `/* comment */` as bare text. Without braces, the comment \
                  syntax is rendered as visible text in the DOM.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};
--
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-index-of",
    description: "Prefer `.indexOf(val)` over `.findIndex(x => x === val)`.",
    remediation: "Replace `.findIndex(x => x === val)` with `.indexOf(val)` for simple equality checks.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-check-types",
    description: "JSDoc types must use the preferred casing and form (e.g. `number` not `Number`).",
    remediation: "Use the lowercase primitive form (`string`, `number`, `boolean`, `object`, `symbol`, `undefined`, `null`) instead of the wrapper type.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/check-types.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "array-callback-without-return",
    description: "Array method callback with block body but no `return` statement.",
    remediation: "Add a `return` statement inside the callback body, or use a concise arrow expression without braces.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-type-encoded-names",
    description: "Identifiers must not encode their type (`strName`, `arrItems`).",
    remediation: "Remove the type prefix. TypeScript's type checker already \
                  tells you the type — encoding it in the name is obsolete \
                  and lies when the type changes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-dollar-replacements",
    description: "Replacement string references a capturing group that does not exist in the regex.",
    remediation: "Fix the replacement reference to match an existing capturing group, or use `$$` to insert a literal dollar sign.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-useless-dollar-replacements.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "test-check-exception",
    description: "`.toThrow()` without specifying what to check.",
    remediation: "Specify the expected error: `.toThrow(TypeError)`, `.toThrow('message')`, or `.toThrow(/regex/)`. Bare `.toThrow()` passes for any error, hiding bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-promise-reject",
    description: "`Promise.reject()` makes error handling harder — prefer returning error values or throwing typed errors.",
    remediation: "Return a Result type, throw a typed error, or use `Promise.resolve()` with an error discriminant.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

--
--
pub const META: RuleMeta = RuleMeta {
    id: "exports-at-top",
    description: "Public API (exports) should appear before private helpers.",
    remediation: "Move all exported (`export` / `pub`) items to the top of \
                  the file. Readers should see the module's public surface \
                  at a glance without scanning through private helpers first.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-boolean-flag-param",
    description: "Boolean flag parameters hide two behaviors behind one signature.",
    remediation: "Split into two named functions. \
                  `sendNotification(msg, isUrgent)` → \
                  `sendUrgentNotification(msg)` + `sendNormalNotification(msg)`. \
                  A ternary or options object is not a fix — the boolean \
                  must disappear from the signature.",
    severity: Severity::Error,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-check-property-names",
    description: "JSDoc `@property` names must not be duplicated.",
    remediation: "Remove or rename duplicate `@property` tags so each property is documented exactly once.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/check-property-names.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "colocated-tests",
    description: "Source file has no colocated test file.",
    remediation: "Create a `.test.ts` or `.spec.ts` file next to the source file with the same base name.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-focused-test",
    description: "`.only` disables every other test in the suite.",
    remediation: "Remove `.only` from the test. Committing a focused test \
                  silently disables the rest of the suite, making CI green \
                  while regressions slip through.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-use-unicode-flag",
    description: "Unicode property escapes (`\\p{...}` / `\\P{...}`) require the `u` or `v` flag.",
    remediation: "Add the `u` flag to the regex: `/\\p{Letter}/u`. Without it, `\\p` is not interpreted as a Unicode property escape.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-non-standard-flag",
    description: "Regex uses a non-standard flag that is not part of the ECMAScript specification.",
    remediation: "Remove the non-standard flag. Standard flags are: d, g, i, m, s, u, v, y.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-non-standard-flag.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-param",
    description: "JSDoc block must document every function parameter with `@param`.",
    remediation: "Add a `@param` tag for each undocumented parameter. Callers rely on JSDoc to understand the API without reading implementation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-conditional-expect",
    description: "`expect()` inside `if`/`switch`/`catch` may silently skip — tests must assert unconditionally.",
    remediation: "Move the `expect()` call out of the conditional branch. \
                  A conditional assertion can silently pass when the branch \
                  is never taken, giving false confidence. Structure the \
                  test so the expected state is deterministic.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-for-loop",
    description: "Use a `for-of` loop instead of this `for` loop.",
    remediation: "Replace `for (let i = 0; i < arr.length; i++)` with \
                  `for (const item of arr)`. If the index is needed, use \
                  `for (const [i, item] of arr.entries())`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-invalid-remove-event-listener",
    description: "`removeEventListener` with an inline function or `.bind()` call never matches the original listener.",
    remediation: "Pass a stable function reference to `removeEventListener` — store the bound/arrow function in a variable first.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unused-collection",
    description: "Collection is populated but never read.",
    remediation: "Either use the collection (iterate, return, pass to a function) or remove the dead code. Populated-but-unread collections indicate logic that was never finished or already removed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-type-encoded-names",
    description: "Identifiers must not encode their type (`strName`, `arrItems`).",
    remediation: "Remove the type prefix. TypeScript's type checker already \
                  tells you the type — encoding it in the name is obsolete \
                  and lies when the type changes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
--
pub const META: RuleMeta = RuleMeta {
    id: "error-message",
    description: "Pass a message to the Error constructor.",
    remediation: "Add a descriptive string message as the first argument to the Error \
                  constructor (or second for AggregateError, third for SuppressedError). \
                  Empty strings and non-string literals are also flagged.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-json-parse-buffer",
    description: "Prefer reading a JSON file as a buffer.",
    remediation: "Remove the `'utf-8'` / `'utf8'` encoding argument from \
                  `fs.readFileSync()` when the result is passed to `JSON.parse()`. \
                  `JSON.parse()` accepts a `Buffer` directly, which avoids an \
                  intermediate string allocation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-abusive-eslint-disable",
    description: "`eslint-disable` without specifying rules silences everything — too broad.",
    remediation: "Specify the exact rules to disable: `eslint-disable-next-line no-console`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "array-callback-without-return",
    description: "Array method callback with block body but no `return` statement.",
    remediation: "Add a `return` statement inside the callback body, or use a concise arrow expression without braces.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-all-duplicated-branches",
    description: "All branches have the same implementation — the conditional is pointless.",
    remediation: "Remove the conditional and keep just the body. Duplicated branches hide that the branching is no longer meaningful.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "import-no-webpack-loader-syntax",
    description: "Webpack loader syntax in imports is forbidden.",
    remediation: "Do not use `!` import syntax to configure webpack loaders. Use webpack config instead.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/no-webpack-loader-syntax.md"),
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-hooks",
    description: "Hooks add implicit shared state between tests.",
    remediation: "Replace hooks with explicit helper functions called in each \
                  test body.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-hooks.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-array-index-key",
    description: "Array indices as React keys break on reorder.",
    remediation: "Use a stable id from the data as the React key. \
                  `items.map(item => <X key={item.id} />)` instead of \
                  `items.map((item, i) => <X key={i} />)`. Index keys \
                  associate DOM state with the wrong item on reorder/filter.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-gratuitous-expression",
    description: "Boolean expression is always true or always false.",
    remediation: "Remove the dead branch. A condition that can never flip is either a bug or leftover from a refactor.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "drizzle-timestamp-with-timezone",
    description: "`timestamp('col')` is timezone-ambiguous.",
    remediation: "Add `{ withTimezone: true }` to every timestamp column. \
                  Bare timestamps are interpreted differently depending \
                  on the server's zone, silently corrupting dates.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "drizzle"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-after-reluctant",
    description: "Reluctant quantifier followed by end-of-pattern or group is useless.",
    remediation: "Remove the `?` from the quantifier — it has no effect when nothing follows it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-confusing-non-null-assertion",
    description: "`a! == b` looks confusingly like `a !== b`.",
    remediation: "Remove the `!` or wrap the left side in parentheses: `(a!) == b`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "colocated-tests",
    description: "Source file has no colocated test file.",
    remediation: "Create a `.test.ts` or `.spec.ts` file next to the source file with the same base name.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unsafe-declaration-merging",
    description: "Unsafe declaration merging between classes and interfaces.",
    remediation: "Rename one of the declarations so they don't merge.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-misused-new",
    description: "Classes use `constructor()`, not `new()`. Interfaces use `new()`, not `constructor()`.",
    remediation: "In a class, rename `new` to `constructor`. In an interface, use `new(): Type` \
                  instead of `constructor(): Type`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-duplicate-hooks",
    description: "Duplicate hooks in a describe block are confusing and error-prone.",
    remediation: "Merge the duplicate hooks into a single hook call.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-duplicate-hooks.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-collection-argument",
    description: "Disallow useless values in `Set`, `Map`, `WeakSet`, or `WeakMap` constructors.",
    remediation: "Remove the empty/null/undefined argument from the collection \
                  constructor. `new Set([])` and `new Map(undefined)` are \
                  equivalent to `new Set()` and `new Map()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "node-callback-return",
    description: "Callback invocations should be followed by a `return`.",
    remediation: "Add `return` before or after calling `callback`/`cb`/`next` to prevent accidental double execution.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/callback-return.md"),
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-classlist-toggle",
    description: "Prefer `Element#classList.toggle()` over conditional `add`/`remove`.",
    remediation: "Replace `if (c) el.classList.add('x') else el.classList.remove('x')` with `el.classList.toggle('x', c)`. The `toggle` method with a force argument is cleaner and avoids conditional branching.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "hono-secure-headers-disabled",
    description: "Security header explicitly disabled in `secureHeaders()`.",
    remediation: "Don't disable security headers. Each one protects against a specific attack vector (HSTS, clickjacking, MIME sniffing, fingerprinting).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-find",
    description: "Prefer `.find(…)` over `.filter(…)[0]` or `.filter(…).at(0)`.",
    remediation: "Replace `.filter(…)[0]` with `.find(…)` to short-circuit on the first match.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-auth-token-in-localstorage",
    description: "Auth tokens in localStorage are XSS-exfiltratable.",
    remediation: "Store auth tokens in httpOnly cookies. The browser \
                  attaches them automatically and JavaScript cannot read \
                  them, so a successful XSS can't steal the session.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};
--

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-import-type-side-effects",
    description: "Inline `type` qualifiers on every specifier leave a side-effect import at runtime.",
    remediation: "Use a top-level `import type { ... }` instead of `import { type A, type B }`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-import-type-side-effects/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-conditional-expect",
    description: "`expect()` inside `if`/`switch`/`catch` may silently skip — tests must assert unconditionally.",
    remediation: "Move the `expect()` call out of the conditional branch. \
                  A conditional assertion can silently pass when the branch \
                  is never taken, giving false confidence. Structure the \
                  test so the expected state is deterministic.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-nested-ternary",
    description: "Nested ternaries are hard to read and easy to misparse.",
    remediation: "Nested ternary — extract to if/else or a named variable for each branch.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
    crate::register_ts_family!(META, typescript)
}
--
pub const META: RuleMeta = RuleMeta {
    id: "boolean-naming",
    description: "Boolean identifiers must start with is/has/should/can/will/did/was.",
    remediation: "Rename to convey the predicate: `ready` → `isReady` (TS) or \
                  `is_ready` (Rust). Use the positive form only — prefer \
                  `!isReady` over `isNotReady`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-timestamp-without-tz",
    description: "`TIMESTAMP` without timezone — use `TIMESTAMPTZ` to avoid timezone bugs.",
    remediation: "Replace `TIMESTAMP` with `TIMESTAMPTZ` (or `TIMESTAMP WITH TIME ZONE`). Without timezone info, the same instant is interpreted differently depending on the server's `timezone` setting.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-style-prop-object",
    description: "The `style` prop expects an object, not a CSS string.",
    remediation: "Use `style={{ color: 'red' }}` instead of `style=\"color: red\"`. \
                  React's `style` prop takes a JavaScript object with camelCase \
                  property names, not a CSS string.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-date-now",
    description: "Prefer `Date.now()` over `new Date().getTime()`, `+new Date()`, or `Number(new Date())`.",
    remediation: "Replace with `Date.now()`. It is clearer, avoids allocating a throwaway `Date` object, and is faster.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "throw-new-error",
    description: "Use `new` when creating an error.",
    remediation: "Replace `throw Error(...)` with `throw new Error(...)`. \
                  Calling Error without `new` is valid but inconsistent and \
                  can confuse readers about whether a new instance is created.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "error-message",
    description: "Pass a message to the Error constructor.",
    remediation: "Add a descriptive string message as the first argument to the Error \
                  constructor (or second for AggregateError, third for SuppressedError). \
                  Empty strings and non-string literals are also flagged.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-empty-collection-use",
    description: "Collection is used before any element is added.",
    remediation: "Populate the collection before reading from it. Iterating an always-empty collection is dead code.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-immediate-mutation",
    description: "Disallow immediate mutation after variable assignment.",
    remediation: "Chain the mutation onto the initialiser: \
                  `const arr = [3,1,2].sort()` instead of declaring then \
                  mutating on the next line. This makes the intent clearer \
                  and avoids an intermediate mutable state.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-top-level-format",
    description: "Zod v4 top-level format helpers are shorter and faster.",
    remediation: "Replace `z.string().email()` with `z.email()`, \
                  `z.string().url()` with `z.url()`, `z.number().int()` with \
                  `z.int()`, and similar chains. Top-level helpers are \
                  tree-shakeable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "zod"],
--
pub const META: RuleMeta = RuleMeta {
    id: "test-check-exception",
    description: "`.toThrow()` without specifying what to check.",
    remediation: "Specify the expected error: `.toThrow(TypeError)`, `.toThrow('message')`, or `.toThrow(/regex/)`. Bare `.toThrow()` passes for any error, hiding bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "factory-di-shape",
    description: "`create*` factory functions should take a single deps object, not individual params.",
    remediation: "Replace individual dependency parameters with a single object: `createService({ db, cache, logger })`. A deps object makes the dependency list extensible without breaking callers and reads as named arguments.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-function-declaration-in-block",
    description: "Function declaration inside a control-flow block has inconsistent hoisting behavior.",
    remediation: "Move the function declaration to the top level, or use a `const fn = () => { ... }` expression instead. Function declarations in blocks are only conditionally hoisted in sloppy mode and forbidden in strict mode by some engines.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-class-literal-property-style",
    description: "Enforce that literals on classes are exposed in a consistent style (fields vs getters).",
    remediation: "Use `readonly` fields for literals instead of trivial getter methods (default), or vice versa.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/class-literal-property-style/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-focused-test",
    description: "`.only` disables every other test in the suite.",
    remediation: "Remove `.only` from the test. Committing a focused test \
                  silently disables the rest of the suite, making CI green \
                  while regressions slip through.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "arguments-order",
    description: "Function arguments appear to be in the wrong order.",
    remediation:
        "Swap the arguments so `expected` comes after `actual`, and `min` comes before `max`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-disable-mustache-escape",
    description: "Disabling template engine HTML escaping (`escapeMarkup = false`) opens XSS vectors.",
    remediation: "Keep HTML escaping enabled. If raw HTML is truly needed, sanitize it explicitly before rendering.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "justify-inaction",
    description: "Empty `catch {}`, `else {}`, or early `return;` without an explaining comment.",
    remediation: "Add a comment on the preceding line explaining why the block is intentionally empty or why the early return is correct. Silent inaction hides bugs — make the intent explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-function-overloads",
    description: "Overload signatures don't constrain the implementation.",
    remediation: "Replace overloads with a union parameter type or a \
                  generic signature. Overloads are purely ambient \
                  declarations — the compiler checks the implementation \
                  against the last signature only, which hides bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-array-index-key",
    description: "Array indices as React keys break on reorder.",
    remediation: "Use a stable id from the data as the React key. \
                  `items.map(item => <X key={item.id} />)` instead of \
                  `items.map((item, i) => <X key={i} />)`. Index keys \
                  associate DOM state with the wrong item on reorder/filter.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-pub-use-glob",
    description: "`pub use foo::*` re-exports invisibly.",
    remediation: "List the re-exports explicitly: `pub use foo::{Bar, Baz};`. \
                  Glob re-exports turn every change in `foo` into a silent \
                  change to your public API.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-expect-expect",
    description: "Test has no assertions — every test should verify behaviour.",
    remediation: "Add at least one `expect()` call inside the test body.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/expect-expect.md"),
    categories: &["testing"],
};

--
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-misused-new",
    description: "Classes use `constructor()`, not `new()`. Interfaces use `new()`, not `constructor()`.",
    remediation: "In a class, rename `new` to `constructor`. In an interface, use `new(): Type` \
                  instead of `constructor(): Type`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-prototype-methods",
    description: "Prefer borrowing methods from the prototype instead of a literal instance.",
    remediation:
        "Replace `{}.hasOwnProperty.call(…)` with `Object.prototype.hasOwnProperty.call(…)`, \
                  `[].slice.call(…)` with `Array.prototype.slice.call(…)`, etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-verb-in-rest-url",
    description: "REST URLs should identify resources, not actions.",
    remediation: "Replace verb-in-URL patterns with HTTP semantics: \
                  `POST /api/orders` to create, `GET /api/orders/:id` to \
                  read, `PATCH /api/orders/:id` to update, \
                  `DELETE /api/orders/:id` to remove.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-access-state-in-setstate",
    description: "`this.state` inside `setState()` reads stale state.",
    remediation: "Use the updater callback form: `this.setState(prevState => ({ \
                  count: prevState.count + 1 }))`. Reading `this.state` inside \
                  `setState` may read a stale value because React batches state \
                  updates.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
--
--

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-strict-equal",
    description: "Prefer `toStrictEqual()` for more predictable deep equality checks.",
    remediation: "Replace `toEqual()` with `toStrictEqual()`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-strict-equal.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "node-no-sync",
    description: "Synchronous Node.js methods block the event loop.",
    remediation: "Use the asynchronous variant (e.g. `readFile` instead of `readFileSync`) or `fs.promises`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-manual-rtl-cleanup",
    description: "Importing `cleanup` from `@testing-library/react` in Vitest causes double cleanup.",
    remediation: "Remove the `cleanup` import and any `afterEach(cleanup)` \
                  call. Vitest with `@testing-library/react` runs cleanup \
                  automatically after each test. Manual cleanup causes \
                  double cleanup which can mask unmount bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-some",
    description: "Prefer `.some(…)` over `.filter(…).length` checks.",
    remediation: "Replace `.filter(…).length > 0` with `.some(…)` — it short-circuits.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unnecessary-array-flat-depth",
    description: "Disallow using `1` as the `depth` argument of `Array#flat()`.",
    remediation: "Remove the argument: `.flat()` defaults to depth 1.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-spread",
    description: "Prefer the spread operator over `Array.from()`, `Array#concat()`, and `Array#slice()`.",
    remediation: "Use `[...x]` instead of `Array.from(x)`, `[...arr, ...other]` instead of `arr.concat(other)`, and `[...arr]` instead of `arr.slice()`. The spread syntax is more idiomatic.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-enum",
    description: "TypeScript enums emit runtime code and don't narrow cleanly.",
    remediation: "Replace `enum` with `const X = { ... } as const satisfies \
                  Record<string, string>` for config, or a discriminated \
                  union with a `type`/`kind` field for tagged data.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-auth-token-in-localstorage",
    description: "Auth tokens in localStorage are XSS-exfiltratable.",
    remediation: "Store auth tokens in httpOnly cookies. The browser \
                  attaches them automatically and JavaScript cannot read \
                  them, so a successful XSS can't steal the session.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-destructuring-assignment",
    description: "Consecutive property accesses on the same object can be destructured.",
    remediation: "Use destructuring: `const { x, y } = obj;` instead of separate `const x = obj.x; const y = obj.y;` declarations. Destructuring is more concise and makes the intent clear.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-incomplete-assertions",
    description: "Assertion chain is missing the actual matcher.",
    remediation: "Complete the assertion with a matcher: `expect(x).toBe(...)`, `.toEqual(...)`, `.toThrow()`, etc. Bare `expect(x);` or `expect(x).not;` tests nothing.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "hono-csrf-missing",
    description: "Mutation routes without CSRF protection.",
    remediation: "Add `import { csrf } from 'hono/csrf'` and `app.use(csrf())` to protect mutation endpoints against cross-site request forgery.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "hono"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-boolean-flag-param",
    description: "Boolean flag parameters hide two behaviors behind one signature.",
    remediation: "Split into two named functions. \
                  `sendNotification(msg, isUrgent)` → \
                  `sendUrgentNotification(msg)` + `sendNormalNotification(msg)`. \
                  A ternary or options object is not a fix — the boolean \
                  must disappear from the signature.",
    severity: Severity::Error,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-reflect-apply",
    description: "Prefer `Reflect.apply(fn, thisArg, args)` over `fn.apply(thisArg, args)`.",
    remediation: "Replace `fn.apply(ctx, args)` with `Reflect.apply(fn, ctx, args)`. \
                  `Reflect.apply` cannot be overridden and makes the intent explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "reduce-initial-value",
    description: "`.reduce()` without initial value throws on empty arrays.",
    remediation: "Always pass a second argument to `.reduce()`: `arr.reduce((acc, x) => acc + x, 0)`. Without it, an empty array causes `TypeError: Reduce of empty array with no initial value`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-global-this",
    description: "Prefer `globalThis` over `window`, `self`, and `global`.",
    remediation: "Replace `window.`, `self.`, or `global.` with `globalThis.`. \
                  `globalThis` is the standard cross-platform way to access the \
                  global object in any JS environment.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "react-style-prop-object",
    description: "The `style` prop expects an object, not a CSS string.",
    remediation: "Use `style={{ color: 'red' }}` instead of `style=\"color: red\"`. \
                  React's `style` prop takes a JavaScript object with camelCase \
                  property names, not a CSS string.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-object-as-default-parameter",
    description: "Do not use an object literal as a default parameter value.",
    remediation: "Use destructuring with individual defaults instead of a \
                  default object literal. `function f({ timeout = 1000 } = {})` \
                  is clearer and avoids the all-or-nothing replacement problem \
                  when a caller passes a partial object.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-unenclosed-multiline-block",
    description: "`if`/`for`/`while` without braces and a multiline body is a bug magnet.",
    remediation: "Always wrap `if`/`for`/`while` bodies in curly braces `{}` when the body is on the next line.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-this-in-sfc",
    description: "`this` has no meaning inside a functional component.",
    remediation: "Remove `this.` references. Functional components don't have a \
                  `this` context — use hooks (`useState`, `useRef`, etc.) instead \
                  of `this.state`, `this.props`, etc.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-string-as-error",
    description: "`Result<T, String>` is stringly-typed and unmatchable.",
    remediation: "Define a proper error enum (use `thiserror::Error` for \
                  the boilerplate) and use it as the `E` parameter. \
                  String errors prevent callers from pattern-matching \
                  failure modes and lose all structured context.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-abusive-eslint-disable",
    description: "`eslint-disable` without specifying rules silences everything — too broad.",
    remediation: "Specify the exact rules to disable: `eslint-disable-next-line no-console`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "node-handle-callback-err",
    description: "Callback error parameter is declared but never used.",
    remediation: "Handle the error parameter (log it, rethrow, or forward). If intentionally unused, prefix with `_`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "factory-di-shape",
    description: "`create*` factory functions should take a single deps object, not individual params.",
    remediation: "Replace individual dependency parameters with a single object: `createService({ db, cache, logger })`. A deps object makes the dependency list extensible without breaking callers and reads as named arguments.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-append",
    description: "Prefer `Node#append()` over `Node#appendChild()`.",
    remediation: "Replace `.appendChild(x)` with `.append(x)`. \
                  `.append()` accepts multiple arguments, strings, and \
                  never returns the appended node (avoiding subtle misuse).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "import-consistent-type-specifier-style",
    description: "Type-only imports should use top-level `import type` syntax.",
    remediation: "Use `import type { Foo }` instead of `import { type Foo }`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/consistent-type-specifier-style.md"),
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "vue-no-reactive-destructure",
    description: "Destructuring `reactive()` breaks reactivity — use `toRefs()` or `ref()`.",
    remediation: "`const { count } = reactive({ count: 0 })` copies the primitive — \
                  `count` is now a plain number, not reactive. Use \
                  `const { count } = toRefs(state)` to get a ref that stays connected, \
                  or use `ref()` directly for each field.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "vue"],
--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-like-wildcard-prefix",
    description: "`LIKE '%...'` prevents index usage — use full-text search instead.",
    remediation: "Replace `LIKE '%term%'` with a TSVECTOR + GIN index and `@@` operator. Leading wildcards force a sequential scan on every row.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-empty-collection-use",
    description: "Collection is used before any element is added.",
    remediation: "Populate the collection before reading from it. Iterating an always-empty collection is dead code.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-non-null-asserted-nullish-coalescing",
    description: "`x! ?? y` is contradictory — `!` asserts non-null, `??` handles null.",
    remediation: "Remove the `!` (let `??` do its job) or remove the `??` (if the value is never null).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-access-state-in-setstate",
    description: "`this.state` inside `setState()` reads stale state.",
    remediation: "Use the updater callback form: `this.setState(prevState => ({ \
                  count: prevState.count + 1 }))`. Reading `this.state` inside \
                  `setState` may read a stale value because React batches state \
                  updates.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-format-in-debug-impl",
    description: "`format!` inside `Debug::fmt` allocates an extra `String` per call.",
    remediation: "Replace `format!(\"...\", x)` with a direct `write!(f, \"...\", x)` \
                  call. `write!` streams into the formatter's writer; \
                  `format!` builds an intermediate `String` that you \
                  immediately throw away.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-function-overloads",
    description: "Overload signatures don't constrain the implementation.",
    remediation: "Replace overloads with a union parameter type or a \
                  generic signature. Overloads are purely ambient \
                  declarations — the compiler checks the implementation \
                  against the last signature only, which hides bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
--
pub const META: RuleMeta = RuleMeta {
    id: "hono-csp-unsafe",
    description: "`unsafe-inline` or `unsafe-eval` in CSP defeats its purpose.",
    remediation: "Use nonces (`NONCE` from `hono/secure-headers`) instead of `unsafe-inline`. Avoid `unsafe-eval` — it enables code injection.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-zero-quantifier",
    description: "Quantifier `{0}` or `{0,0}` matches nothing — the pattern is likely a mistake.",
    remediation: "Remove the quantified sub-expression or fix the quantifier.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-pub-use-glob",
    description: "`pub use foo::*` re-exports invisibly.",
    remediation: "List the re-exports explicitly: `pub use foo::{Bar, Baz};`. \
                  Glob re-exports turn every change in `foo` into a silent \
                  change to your public API.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-set-has",
    description:
        "Prefer `Set#has()` over `Array#includes()` when checking for existence or non-existence.",
    remediation: "Convert the array to a `Set` and use `.has()` instead of \
                  `.includes()`. `Array#includes()` is O(n) per call; \
                  `Set#has()` is O(1). This matters when the check is inside \
                  a loop or called repeatedly.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-property",
    description: "A `@typedef` with `@property` tags must document every property.",
    remediation: "Add `@property` tags for each property of the typedef.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-property.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-undefined-argument",
    description: "Passing `undefined` as a function argument is pointless.",
    remediation: "Omit the argument instead of passing `undefined` explicitly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-mock-fetch-directly",
    description:
        "Mocking `fetch`/`axios` directly couples tests to the HTTP client implementation.",
    remediation: "Use MSW (`msw`) to intercept at the network level instead \
                  of `vi.mock('axios')` or `global.fetch = vi.fn()`. MSW \
                  handlers are reusable, work with any HTTP client, and \
                  test real request/response cycles. Switching HTTP clients \
                  won't break your tests.",
    severity: Severity::Warning,
--

--
pub const META: RuleMeta = RuleMeta {
    id: "no-nested-functions",
    description: "Deeply nested function declarations reduce readability.",
    remediation: "Extract inner functions to module scope or pass them as parameters. Nesting beyond 2 levels makes code hard to follow and test.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-spread",
    description: "Prefer the spread operator over `Array.from()`, `Array#concat()`, and `Array#slice()`.",
    remediation: "Use `[...x]` instead of `Array.from(x)`, `[...arr, ...other]` instead of `arr.concat(other)`, and `[...arr]` instead of `arr.slice()`. The spread syntax is more idiomatic.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "no-function-declaration-in-block",
    description: "Function declaration inside a control-flow block has inconsistent hoisting behavior.",
    remediation: "Move the function declaration to the top level, or use a `const fn = () => { ... }` expression instead. Function declarations in blocks are only conditionally hoisted in sloppy mode and forbidden in strict mode by some engines.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-nth-methods",
    description: "`.first()`, `.last()`, `.nth()` create fragile locators.",
    remediation: "Use a more specific locator (e.g. `getByRole`, `getByTestId`) \
                  instead of positional methods.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-nth-methods.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-optional",
    description: "`prop?: Type | undefined` — the `?` already implies `| undefined`.",
    remediation: "Remove `| undefined` from the type, or remove the `?` from the property name.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-set-has",
    description:
        "Prefer `Set#has()` over `Array#includes()` when checking for existence or non-existence.",
    remediation: "Convert the array to a `Set` and use `.has()` instead of \
                  `.includes()`. `Array#includes()` is O(n) per call; \
                  `Set#has()` is O(1). This matters when the check is inside \
                  a loop or called repeatedly.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-useless-empty-export",
    description: "`export {}` is unnecessary when the file already has other exports.",
    remediation: "Remove the `export {}` statement.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-async-constructor",
    description: "Constructors cannot be `async` — they must return the instance, not a Promise.",
    remediation: "Use a static async factory method instead: `static async create() { ... return new MyClass(); }`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "arguments-order",
    description: "Function arguments appear to be in the wrong order.",
    remediation:
        "Swap the arguments so `expected` comes after `actual`, and `min` comes before `max`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-throw",
    description: "Never throw — Result<T, E> surfaces errors as values.",
    remediation: "Use Result<T, E> instead of throw — surface errors as values, \
                  not exceptions. Callers can't see thrown errors in the type signature.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
    crate::register_ts_family_with_clippy_marker!(META, typescript, "clippy::panic")
--
pub const META: RuleMeta = RuleMeta {
    id: "no-inline-param-type",
    description: "Inline object types in parameters resist reuse and refactoring.",
    remediation: "Extract the inline type to a named `type` declaration \
                  above the function. A named type has an identity, can be \
                  shared across call sites, and shows up in IDE hover.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-prototype-methods",
    description: "Prefer borrowing methods from the prototype instead of a literal instance.",
    remediation:
        "Replace `{}.hasOwnProperty.call(…)` with `Object.prototype.hasOwnProperty.call(…)`, \
                  `[].slice.call(…)` with `Array.prototype.slice.call(…)`, etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-append",
    description: "Prefer `Node#append()` over `Node#appendChild()`.",
    remediation: "Replace `.appendChild(x)` with `.append(x)`. \
                  `.append()` accepts multiple arguments, strings, and \
                  never returns the appended node (avoiding subtle misuse).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "reduce-initial-value",
    description: "`.reduce()` without initial value throws on empty arrays.",
    remediation: "Always pass a second argument to `.reduce()`: `arr.reduce((acc, x) => acc + x, 0)`. Without it, an empty array causes `TypeError: Reduce of empty array with no initial value`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-ecb-mode",
    description: "ECB cipher mode is insecure — identical plaintext blocks produce identical ciphertext.",
    remediation: "Use CBC, CTR, or GCM mode instead of ECB. ECB does not provide semantic security because it encrypts identical blocks to the same ciphertext, leaking patterns.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
--
pub const META: RuleMeta = RuleMeta {
    id: "no-unthrown-error",
    description: "`new Error(...)` is created but never thrown, returned, or assigned.",
    remediation: "Add `throw` before `new Error(...)`, or assign/return it.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-async-constructor",
    description: "Constructors cannot be `async` — they must return the instance, not a Promise.",
    remediation: "Use a static async factory method instead: `static async create() { ... return new MyClass(); }`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-useless-empty-export",
    description: "`export {}` is unnecessary when the file already has other exports.",
    remediation: "Remove the `export {}` statement.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-some",
    description: "Prefer `.some(…)` over `.filter(…).length` checks.",
    remediation: "Replace `.filter(…).length > 0` with `.some(…)` — it short-circuits.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-format-in-debug-impl",
    description: "`format!` inside `Debug::fmt` allocates an extra `String` per call.",
    remediation: "Replace `format!(\"...\", x)` with a direct `write!(f, \"...\", x)` \
                  call. `write!` streams into the formatter's writer; \
                  `format!` builds an intermediate `String` that you \
                  immediately throw away.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-optional-assertion",
    description: "Assertion inside an optional group is effectively ignored and does not change the pattern.",
    remediation: "Remove the assertion or change the parent quantifier so the assertion is always evaluated.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-optional-assertion.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "node-handle-callback-err",
    description: "Callback error parameter is declared but never used.",
    remediation: "Handle the error parameter (log it, rethrow, or forward). If intentionally unused, prefix with `_`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "node-no-sync",
    description: "Synchronous Node.js methods block the event loop.",
    remediation: "Use the asynchronous variant (e.g. `readFile` instead of `readFileSync`) or `fs.promises`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-element-handle",
    description: "`page.$()` / `page.$$()` return ElementHandles, which are deprecated in favor of Locators.",
    remediation: "Replace `page.$('selector')` with `page.locator('selector')` \
                  and `page.$$('selector')` with \
                  `page.locator('selector').all()`. Locators auto-wait and \
                  retry, while ElementHandles are stale references that \
                  break on re-renders.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "no-ecb-mode",
    description: "ECB cipher mode is insecure — identical plaintext blocks produce identical ciphertext.",
    remediation: "Use CBC, CTR, or GCM mode instead of ECB. ECB does not provide semantic security because it encrypts identical blocks to the same ciphertext, leaking patterns.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-undefined-argument",
    description: "Passing `undefined` as a function argument is pointless.",
    remediation: "Omit the argument instead of passing `undefined` explicitly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-this-in-sfc",
    description: "`this` has no meaning inside a functional component.",
    remediation: "Remove `this.` references. Functional components don't have a \
                  `this` context — use hooks (`useState`, `useRef`, etc.) instead \
                  of `this.state`, `this.props`, etc.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-rejects",
    description: "Async functions that reject must document rejections with `@rejects`.",
    remediation: "Add a `@rejects` tag documenting what the function rejects with.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-rejects.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-math-min-max",
    description: "Prefer `Math.min()`/`Math.max()` over comparison ternaries.",
    remediation: "Replace `value > max ? max : value` with `Math.min(value, max)` \
                  (or `Math.max` for the inverse pattern).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-destructuring-assignment",
    description: "Consecutive property accesses on the same object can be destructured.",
    remediation: "Use destructuring: `const { x, y } = obj;` instead of separate `const x = obj.x; const y = obj.y;` declarations. Destructuring is more concise and makes the intent clear.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-hook-setter-in-body",
    description: "`useState` setter called directly in component body causes infinite re-renders.",
    remediation: "Move the setter call inside `useEffect`, `useCallback`, or an event handler.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-non-null-asserted-nullish-coalescing",
    description: "`x! ?? y` is contradictory — `!` asserts non-null, `??` handles null.",
    remediation: "Remove the `!` (let `??` do its job) or remove the `??` (if the value is never null).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "api-first",
    description: "Route handler files should define an API schema.",
    remediation: "Define the API schema before the handler using `z.object`, `createRoute`, or `zodValidator`. API-first design ensures the contract is documented and validated before implementation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-invalid-fetch-options",
    description: "`fetch()` / `new Request()` with `body` on a GET or HEAD request is invalid.",
    remediation: "Remove the `body` property or change the method to POST/PUT/PATCH.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-global-this",
    description: "Prefer `globalThis` over `window`, `self`, and `global`.",
    remediation: "Replace `window.`, `self.`, or `global.` with `globalThis.`. \
                  `globalThis` is the standard cross-platform way to access the \
                  global object in any JS environment.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-object-as-default-parameter",
    description: "Do not use an object literal as a default parameter value.",
    remediation: "Use destructuring with individual defaults instead of a \
                  default object literal. `function f({ timeout = 1000 } = {})` \
                  is clearer and avoids the all-or-nothing replacement problem \
                  when a caller passes a partial object.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-text-content",
    description: "Prefer `.textContent` over `.innerText`.",
    remediation: "Replace `.innerText` with `.textContent`. \
                  `.textContent` is faster (no layout reflow), works on all \
                  node types, and returns text from hidden elements too.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-regexp-exec",
    description: "`.match(/regex/)` is slower than `regex.exec(string)` for non-global regexps.",
    remediation: "Use `regex.exec(string)` instead of `string.match(regex)`. For non-global regular expressions, `RegExp.prototype.exec()` is faster and returns the same result.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "expression-complexity",
    description: "Overly complex expression with too many logical/conditional operators.",
    remediation: "Extract parts of the expression into named intermediate variables. Lines with 4+ logical/conditional operators are hard to read and reason about.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "symmetric-pairs",
    description: "Exported function has no symmetric counterpart (get/set, add/remove, open/close, start/stop, create/delete).",
    remediation: "Add the missing counterpart or remove the export if the pair is intentionally incomplete.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-switch-over-chained-if",
    description: "Long if/else-if chains should be switch statements.",
    remediation: "Convert a 4+ branch if/else-if chain into a `switch` \
                  statement. Switch makes the discriminant obvious and \
                  lets TypeScript warn on missing cases for union types.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
--
pub const META: RuleMeta = RuleMeta {
    id: "vue-no-reactive-destructure",
    description: "Destructuring `reactive()` breaks reactivity — use `toRefs()` or `ref()`.",
    remediation: "`const { count } = reactive({ count: 0 })` copies the primitive — \
                  `count` is now a plain number, not reactive. Use \
                  `const { count } = toRefs(state)` to get a ref that stays connected, \
                  or use `ref()` directly for each field.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "vue"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-instanceof-builtins",
    description: "Avoid `instanceof` for built-in types — it fails across realms.",
    remediation: "Use `Array.isArray(x)` instead of `x instanceof Array`. \
                  For errors, check the `name` property or use `Error.isError()`. \
                  `instanceof` breaks across iframes, VMs, and module boundaries.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-hardcoded-secret",
    description: "Hardcoded secrets get exfiltrated from source control.",
    remediation: "Move the secret to an environment variable or secret \
                  store. Rotate the secret immediately — assume it is \
                  already compromised if it reached a commit.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-react-setstate",
    description: "Calling a `useState` setter with its own state value is a no-op.",
    remediation: "Remove the useless `setState` call or pass a different value. `setX(x)` triggers a re-render but does not change state.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-class-inheritance",
    description: "Class inheritance (`extends`) creates tight coupling — prefer composition over inheritance.",
    remediation: "Use composition, mixins, or dependency injection instead of class inheritance.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-consistent-type-assertions",
    description: "Enforce consistent type assertion style (`as T` vs `<T>`).",
    remediation: "Use `as T` syntax instead of angle-bracket `<T>` assertions for consistency.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/consistent-type-assertions/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-common-grab-bag",
    description: "Grab-bag filenames magnetize unrelated code.",
    remediation: "Rename the file to describe what it actually owns. \
                  `common`/`utils`/`helpers`/`shared`/`misc` are magnet names \
                  that attract unrelated code over time.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-yields-check",
    description: "A `@yields` tag must be present when a generator function has a `yield` statement.",
    remediation: "Add a `@yields` tag since this generator contains `yield` expressions.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-yields-check.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "api-first",
    description: "Route handler files should define an API schema.",
    remediation: "Define the API schema before the handler using `z.object`, `createRoute`, or `zodValidator`. API-first design ensures the contract is documented and validated before implementation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "hono-csrf-missing",
    description: "Mutation routes without CSRF protection.",
    remediation: "Add `import { csrf } from 'hono/csrf'` and `app.use(csrf())` to protect mutation endpoints against cross-site request forgery.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "hono"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-hardcoded-ip",
    description: "Hardcoded IP address — move to configuration.",
    remediation: "Move the IP to an environment variable or config file. Hardcoded IPs break on deploy and leak infrastructure details into source control.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-invalid-this",
    description: "`this` used outside a class or class-like object is likely a bug.",
    remediation: "Move the code into a class method, or use an explicit parameter instead of `this`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-invalid-this"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-number-properties",
    description: "Prefer `Number.isNaN()`, `Number.parseInt()`, etc. over global equivalents.",
    remediation: "Replace global `isNaN()`, `isFinite()`, `parseInt()`, `parseFloat()`, `NaN`, \
                  and `Infinity` with their `Number.*` equivalents. The `Number` methods are \
                  stricter (no implicit coercion) and the properties are unambiguous.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-zero-quantifier",
    description: "Quantifier `{0}` or `{0,0}` matches nothing — the pattern is likely a mistake.",
    remediation: "Remove the quantified sub-expression or fix the quantifier.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-while",
    description: "`for (;;)` or `for (;cond;)` without init/update — use `while` instead.",
    remediation: "Replace `for (;;)` with `while (true)` and `for (;condition;)` with `while (condition)`. The `for` form hides intent when init and update are unused.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "symmetric-pairs",
    description: "Exported function has no symmetric counterpart (get/set, add/remove, open/close, start/stop, create/delete).",
    remediation: "Add the missing counterpart or remove the export if the pair is intentionally incomplete.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unenclosed-multiline-block",
    description: "`if`/`for`/`while` without braces and a multiline body is a bug magnet.",
    remediation: "Always wrap `if`/`for`/`while` bodies in curly braces `{}` when the body is on the next line.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "nested-control-flow",
    description: "Deeply nested control flow (depth > 3) is hard to read and maintain.",
    remediation: "Extract inner blocks into separate functions, use early returns or guard clauses to reduce nesting.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "hono-cookie-no-secure",
    description: "Cookie set without `secure` — sent over unencrypted HTTP.",
    remediation: "Add `secure: true` to cookie options so the cookie is only sent over HTTPS.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "hono"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-nested-functions",
    description: "Deeply nested function declarations reduce readability.",
    remediation: "Extract inner functions to module scope or pass them as parameters. Nesting beyond 2 levels makes code hard to follow and test.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-abbreviated-names",
    description: "Identifier contains a banned abbreviation.",
    remediation: "Use the full word: `usr` → `user`, `cfg` → `config`, \
                  `btn` → `button`. Editors auto-complete; readers don't.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
    crate::register_ts_family_with_rust!(META, typescript, rust)
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-bool-return-from-fallible",
    description: "Action functions return `Result`, not `bool`.",
    remediation: "Change the return type to `Result<T, E>` (use `()` for \
                  T if there's no payload). A bool tells the caller \
                  something failed but not why — they can't handle the \
                  error specifically, only choose to give up or retry blindly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-hardcoded-ip",
    description: "Hardcoded IP address — move to configuration.",
    remediation: "Move the IP to an environment variable or config file. Hardcoded IPs break on deploy and leak infrastructure details into source control.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-like-wildcard-prefix",
    description: "`LIKE '%...'` prevents index usage — use full-text search instead.",
    remediation: "Replace `LIKE '%term%'` with a TSVECTOR + GIN index and `@@` operator. Leading wildcards force a sequential scan on every row.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-invisible-character",
    description: "Invisible Unicode characters in regex (zero-width joiners, soft hyphens, etc.) are hard to spot and usually unintended.",
    remediation: "Use explicit Unicode escapes (`\\u{200D}`) instead of embedding invisible characters directly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-os-command",
    description: "Shell command execution (`exec`, `spawn`, `child_process`) is a command-injection vector.",
    remediation: "Avoid shelling out when a library or built-in API exists. If unavoidable, never interpolate user input — use `execFile` with an argument array and validate inputs.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-class-inheritance",
    description: "Class inheritance (`extends`) creates tight coupling — prefer composition over inheritance.",
    remediation: "Use composition, mixins, or dependency injection instead of class inheritance.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-ban-ts-comment",
    description: "`@ts-ignore` and `@ts-nocheck` suppress compiler errors and hide bugs.",
    remediation: "Fix the underlying type error, or use `@ts-expect-error` with a description.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/ban-ts-comment/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-tags",
    description: "JSDoc comments must include specified required tags.",
    remediation: "Add the required JSDoc tags (e.g. `@param`, `@returns`) to the JSDoc comment.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-tags.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-serde-deny-unknown-fields",
    description: "Deserialize-derive structs need `#[serde(deny_unknown_fields)]`.",
    remediation: "Add `#[serde(deny_unknown_fields)]` above the struct \
                  definition. Without it, typos in input files or API \
                  payloads deserialize silently — fields the type doesn't \
                  know about are dropped, and the user finds out later.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "node-no-process-env",
    description: "Direct use of `process.env` is discouraged.",
    remediation: "Centralize environment access in a config module instead of scattering `process.env` reads.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/no-process-env.md"),
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-inverted-boolean-check",
    description: "`!a === b` negates `a` before comparing — likely meant `a !== b`.",
    remediation: "The `!` operator binds tighter than `===`/`!==`, so `!a === b` is `(!a) === b`, not `!(a === b)`. Use `a !== b` or wrap explicitly: `!(a === b)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "nested-control-flow",
    description: "Deeply nested control flow (depth > 3) is hard to read and maintain.",
    remediation: "Extract inner blocks into separate functions, use early returns or guard clauses to reduce nesting.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "max-function-lines",
    description: "Functions longer than 30 lines mix abstraction levels.",
    remediation: "Function exceeds 30 lines. Extract a named helper for the \
                  tail of the body — one level of abstraction per function.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
    crate::register_ts_family_with_clippy_marker!(META, typescript, "clippy::too_many_lines")
--
pub const META: RuleMeta = RuleMeta {
    id: "no-await-in-promise-methods",
    description: "Promise in `Promise.all/race/any/allSettled()` should not be awaited.",
    remediation: "Remove the `await` keyword from array elements passed to Promise methods. \
                  Awaiting inside the array serializes the calls, defeating the purpose of \
                  `Promise.all()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-conflicting-classes",
    description: "Mutually exclusive Tailwind classes produce unpredictable styles.",
    remediation: "Keep only the intended utility. For example, `p-4 p-6` — \
                  remove one of the two padding values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "hono-csp-unsafe",
    description: "`unsafe-inline` or `unsafe-eval` in CSP defeats its purpose.",
    remediation: "Use nonces (`NONCE` from `hono/secure-headers`) instead of `unsafe-inline`. Avoid `unsafe-eval` — it enables code injection.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-type-error",
    description: "Use `TypeError` instead of `Error` in type-checking conditions.",
    remediation: "When throwing inside an `if` that performs a type check \
                  (typeof, instanceof, Array.isArray, etc.), use `new TypeError()` \
                  instead of `new Error()` to signal the caller passed a wrong type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-os-command",
    description: "Shell command execution (`exec`, `spawn`, `child_process`) is a command-injection vector.",
    remediation: "Avoid shelling out when a library or built-in API exists. If unavoidable, never interpolate user input — use `execFile` with an argument array and validate inputs.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-common-grab-bag",
    description: "Grab-bag filenames magnetize unrelated code.",
    remediation: "Rename the file to describe what it actually owns. \
                  `common`/`utils`/`helpers`/`shared`/`misc` are magnet names \
                  that attract unrelated code over time.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-pseudo-random",
    description: "`Math.random()` is not cryptographically secure.",
    remediation: "Use `crypto.randomUUID()` or `crypto.getRandomValues()` instead of `Math.random()` for security-sensitive contexts.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-mock-fetch-directly",
    description:
        "Mocking `fetch`/`axios` directly couples tests to the HTTP client implementation.",
    remediation: "Use MSW (`msw`) to intercept at the network level instead \
                  of `vi.mock('axios')` or `global.fetch = vi.fn()`. MSW \
                  handlers are reusable, work with any HTTP client, and \
                  test real request/response cycles. Switching HTTP clients \
                  won't break your tests.",
    severity: Severity::Warning,
--
pub const META: RuleMeta = RuleMeta {
    id: "no-multi-op-oneliner",
    description: "Dense one-liners with many chained operators resist review.",
    remediation: "Extract intermediate named variables. Each step of the \
                  expression should have a name that says what it represents \
                  — `activeItems`, `prices`, `subtotal`, `total`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-impl-debug-on-public-types",
    description: "Public structs and enums must derive `Debug`.",
    remediation: "Add `#[derive(Debug)]` (or `#[derive(Debug, …)]`) above \
                  the type definition. Every public type should be loggable \
                  for free, and consumers shouldn't have to wrap your type \
                  to get a `Debug` impl. If a field can't be Debug (e.g. a \
                  closure), implement `Debug` by hand instead.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "no-inverted-boolean-check",
    description: "`!a === b` negates `a` before comparing — likely meant `a !== b`.",
    remediation: "The `!` operator binds tighter than `===`/`!==`, so `!a === b` is `(!a) === b`, not `!(a === b)`. Use `a !== b` or wrap explicitly: `!(a === b)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-iframe-missing-sandbox",
    description: "`<iframe>` without a `sandbox` attribute is a security risk.",
    remediation: "Add a `sandbox` attribute to the `<iframe>`. The `sandbox` \
                  attribute restricts the iframe's capabilities (scripts, forms, \
                  popups) and prevents it from accessing the parent page.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-thread-sleep-in-async",
    description: "`std::thread::sleep` from `async fn` blocks the runtime.",
    remediation: "Replace `std::thread::sleep(d)` with `tokio::time::sleep(d).await` \
                  (or your runtime's equivalent). The async version yields the \
                  worker thread back to the runtime instead of parking it.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "node-no-mixed-requires",
    description: "`require` calls should not be mixed with regular variable declarations.",
    remediation: "Separate `require()` declarations from non-require variable declarations.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/no-mixed-requires.md"),
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-prefer-char-class",
    description: "Single-character alternations should use a character class.",
    remediation: "Replace `a|b|c` with `[abc]`. Character classes are more readable and often faster than alternation for single characters.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-ignored-exceptions",
    description: "Empty `catch` block silently swallows exceptions.",
    remediation: "At minimum, log the error or re-throw it. Silent catch blocks hide bugs and make debugging extremely difficult. If intentional, add an explanatory comment.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-weak-cipher",
    description: "Weak ciphers (DES, RC4, RC2, Blowfish) are cryptographically broken.",
    remediation: "Use AES-256-GCM or ChaCha20-Poly1305 instead of legacy ciphers.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-sort-tags",
    description: "JSDoc tags must follow canonical order: `@param` before `@returns` before `@throws` before `@example`.",
    remediation: "Reorder the tags: `@param` first, then `@returns`, then `@throws`, then `@example`. Consistent ordering makes JSDoc blocks scannable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-returns-check",
    description: "`@returns` on a function that never returns a value.",
    remediation: "Remove the `@returns` tag — the function is void. A stale `@returns` misleads callers into expecting a return value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-iframe-missing-sandbox",
    description: "`<iframe>` without a `sandbox` attribute is a security risk.",
    remediation: "Add a `sandbox` attribute to the `<iframe>`. The `sandbox` \
                  attribute restricts the iframe's capabilities (scripts, forms, \
                  popups) and prevents it from accessing the parent page.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-optional",
    description: "`prop?: Type | undefined` — the `?` already implies `| undefined`.",
    remediation: "Remove `| undefined` from the type, or remove the `?` from the property name.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-implicit-deps",
    description: "Import of a bare specifier that is not a known Node.js builtin — may be an unlisted dependency.",
    remediation: "Ensure the package is listed in `package.json` dependencies. Bare specifier imports that are neither relative paths nor Node.js builtins may break when not explicitly installed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-checked-requires-onchange",
    description: "`checked` prop without `onChange` or `readOnly` makes the input uncontrollable.",
    remediation: "Add an `onChange` handler or `readOnly` prop. Without either, \
                  React renders a frozen checkbox/radio that the user cannot \
                  interact with, and emits a console warning.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "react-void-dom-elements-no-children",
    description: "Void HTML elements like `<br>`, `<img>`, `<input>` cannot have children.",
    remediation: "Remove children or `children`/`dangerouslySetInnerHTML` props \
                  from void elements. These elements are self-closing by spec — \
                  `<br />`, `<img />`, etc.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-inconsistent-returns",
    description: "Function has inconsistent returns — some paths return a value, others return nothing.",
    remediation: "Ensure every return path either returns a value or returns nothing. Mixing `return expr;` with bare `return;` or implicit returns is confusing.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-returns-check",
    description: "`@returns` on a function that never returns a value.",
    remediation: "Remove the `@returns` tag — the function is void. A stale `@returns` misleads callers into expecting a return value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-throw",
    description: "Never throw — Result<T, E> surfaces errors as values.",
    remediation: "Use Result<T, E> instead of throw — surface errors as values, \
                  not exceptions. Callers can't see thrown errors in the type signature.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
    crate::register_ts_family_with_clippy_marker!(META, typescript, "clippy::panic")
--
pub const META: RuleMeta = RuleMeta {
    id: "no-abbreviated-names",
    description: "Identifier contains a banned abbreviation.",
    remediation: "Use the full word: `usr` → `user`, `cfg` → `config`, \
                  `btn` → `button`. Editors auto-complete; readers don't.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
    crate::register_ts_family_with_rust!(META, typescript, rust)
--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-select-star",
    description: "`SELECT *` wastes bandwidth and prevents covering indexes.",
    remediation: "List columns explicitly: `SELECT id, name, email` instead of `SELECT *`. Explicit columns enable index-only scans and make the API contract visible.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-alternative",
    description: "Empty alternative in regex matches empty string and is likely a mistake.",
    remediation: "Remove the leading, trailing, or consecutive `|` in the regex pattern.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-duplicate-imports",
    description: "Multiple import statements from the same module — merge them.",
    remediation: "Combine all imports from the same module into a single import statement.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
--
pub const META: RuleMeta = RuleMeta {
    id: "no-unthrown-error",
    description: "`new Error(...)` is created but never thrown, returned, or assigned.",
    remediation: "Add `throw` before `new Error(...)`, or assign/return it.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-dynamic-class",
    description: "Dynamic Tailwind classes are purged from the stylesheet.",
    remediation: "Use a static map instead of string interpolation: \
                  `const colors = { blue: 'bg-blue-500', red: 'bg-red-500' }; \
                  colors[color]`. Tailwind's purge only sees full static \
                  strings, so `bg-${color}-500` never ships.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["css", "tailwind"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-serde-deny-unknown-fields",
    description: "Deserialize-derive structs need `#[serde(deny_unknown_fields)]`.",
    remediation: "Add `#[serde(deny_unknown_fields)]` above the struct \
                  definition. Without it, typos in input files or API \
                  payloads deserialize silently — fields the type doesn't \
                  know about are dropped, and the user finds out later.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-unnecessary-await",
    description: "Do not `await` non-promise values.",
    remediation: "Remove the unnecessary `await` — literals, arrays, functions, \
                  and other non-thenable values resolve synchronously and the \
                  `await` just adds confusion.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-large-tuple-return",
    description: "Function return tuples with 3+ elements should be named structs.",
    remediation: "Replace `fn f() -> (A, B, C)` with `fn f() -> Result { … }` \
                  where `Result` is a named struct holding the same fields. \
                  Tuples force positional reasoning at every call site and \
                  make refactors impossible.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "migration-needs-lock-timeout",
    description: "DDL migration without `SET lock_timeout` risks write queue pileups.",
    remediation: "Add `SET lock_timeout = '5s';` at the top of every DDL migration. Without it, an ALTER TABLE on a busy table queues all writes behind the lock indefinitely.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "migrations"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-await-in-promise-methods",
    description: "Promise in `Promise.all/race/any/allSettled()` should not be awaited.",
    remediation: "Remove the `await` keyword from array elements passed to Promise methods. \
                  Awaiting inside the array serializes the calls, defeating the purpose of \
                  `Promise.all()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-invisible-character",
    description: "Invisible Unicode characters in regex (zero-width joiners, soft hyphens, etc.) are hard to spot and usually unintended.",
    remediation: "Use explicit Unicode escapes (`\\u{200D}`) instead of embedding invisible characters directly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-sync-io-in-async",
    description: "Synchronous I/O calls inside `async fn` block the runtime.",
    remediation: "Replace `std::fs::*` with `tokio::fs::*`, `std::net::TcpStream::*` \
                  with `tokio::net::TcpStream::*`, etc. If no async equivalent \
                  exists, wrap the call in `tokio::task::spawn_blocking(|| ...)` \
                  so it runs on the dedicated blocking pool.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-param-description",
    description: "Every `@param` tag must include a description.",
    remediation: "Add a description after the parameter name so readers know the purpose of the argument.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-param-description.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-non-null-asserted-optional-chain",
    description: "Non-null assertion after optional chain contradicts its purpose.",
    remediation: "Remove the `!` — the optional chain already handles the nullish case.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-sync-io-in-async",
    description: "Synchronous I/O calls inside `async fn` block the runtime.",
    remediation: "Replace `std::fs::*` with `tokio::fs::*`, `std::net::TcpStream::*` \
                  with `tokio::net::TcpStream::*`, etc. If no async equivalent \
                  exists, wrap the call in `tokio::task::spawn_blocking(|| ...)` \
                  so it runs on the dedicated blocking pool.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-to-be",
    description: "Use `toBe()` for primitives — `toEqual` does unnecessary deep comparison.",
    remediation: "Replace `toEqual(primitive)` with `toBe(primitive)`. \
                  Use `toBeNull()`, `toBeUndefined()`, `toBeNaN()`, `toBeDefined()` \
                  for their respective values.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-to-be.md"),
    categories: &["testing"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-process-exit",
    description: "`process.exit()` terminates abruptly — throw an error instead.",
    remediation: "Replace `process.exit()` with `throw new Error(...)`. Only use `process.exit()` in CLI entry points.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-group",
    description: "Empty capturing group `()` is likely a mistake.",
    remediation: "Remove the empty group or add a pattern inside it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-pseudo-random",
    description: "`Math.random()` is not cryptographically secure.",
    remediation: "Use `crypto.randomUUID()` or `crypto.getRandomValues()` instead of `Math.random()` for security-sensitive contexts.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-nested-assignment",
    description: "Assignment inside a condition or sub-expression is likely a bug.",
    remediation: "Move the assignment before the condition: `x = value; if (x) { ... }`. If intentional, use a separate statement.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-process-exit",
    description: "`process.exit()` terminates abruptly — throw an error instead.",
    remediation: "Replace `process.exit()` with `throw new Error(...)`. Only use `process.exit()` in CLI entry points.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-element-handle",
    description: "`page.$()` / `page.$$()` return ElementHandles, which are deprecated in favor of Locators.",
    remediation: "Replace `page.$('selector')` with `page.locator('selector')` \
                  and `page.$$('selector')` with \
                  `page.locator('selector').all()`. Locators auto-wait and \
                  retry, while ElementHandles are stale references that \
                  break on re-renders.",
    severity: Severity::Warning,
    doc_url: None,
--

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-prefer-predefined-assertion",
    description: "Lookaround assertion can be replaced with a simpler predefined assertion like `\\b` or `^`/`$`.",
    remediation: "Replace the lookaround with the equivalent predefined assertion.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/prefer-predefined-assertion.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-thread-sleep-in-async",
    description: "`std::thread::sleep` from `async fn` blocks the runtime.",
    remediation: "Replace `std::thread::sleep(d)` with `tokio::time::sleep(d).await` \
                  (or your runtime's equivalent). The async version yields the \
                  worker thread back to the runtime instead of parking it.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-implicit-deps",
    description: "Import of a bare specifier that is not a known Node.js builtin — may be an unlisted dependency.",
    remediation: "Ensure the package is listed in `package.json` dependencies. Bare specifier imports that are neither relative paths nor Node.js builtins may break when not explicitly installed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-reflect-apply",
    description: "Prefer `Reflect.apply(fn, thisArg, args)` over `fn.apply(thisArg, args)`.",
    remediation: "Replace `fn.apply(ctx, args)` with `Reflect.apply(fn, ctx, args)`. \
                  `Reflect.apply` cannot be overridden and makes the intent explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "import-no-named-export",
    description: "Named exports are not allowed.",
    remediation: "Use a default export instead of named exports.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/no-named-export.md"),
    categories: &["imports"],
};

--
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-math-min-max",
    description: "Prefer `Math.min()`/`Math.max()` over comparison ternaries.",
    remediation: "Replace `value > max ? max : value` with `Math.min(value, max)` \
                  (or `Math.max` for the inverse pattern).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-duplicate-imports",
    description: "Multiple import statements from the same module — merge them.",
    remediation: "Combine all imports from the same module into a single import statement.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-select-star",
    description: "`SELECT *` wastes bandwidth and prevents covering indexes.",
    remediation: "List columns explicitly: `SELECT id, name, email` instead of `SELECT *`. Explicit columns enable index-only scans and make the API contract visible.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-bidi-characters",
    description: "Invisible Unicode bidi control characters can be used in trojan-source attacks to disguise malicious code.",
    remediation: "Remove the bidi control character. If directional formatting is needed, use visible markup instead.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-multiple-spaces",
    description: "Multiple consecutive spaces in regex are hard to read and count.",
    remediation: "Use a quantifier: ` {2}` or `\\s{2,}` instead of multiple spaces.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-string-as-error",
    description: "`Result<T, String>` is stringly-typed and unmatchable.",
    remediation: "Define a proper error enum (use `thiserror::Error` for \
                  the boilerplate) and use it as the `E` parameter. \
                  String errors prevent callers from pattern-matching \
                  failure modes and lose all structured context.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-module",
    description: "Prefer ESM (`import`/`export`) over CommonJS (`require`/`module.exports`).",
    remediation: "Replace `require()` with `import`, `module.exports` / \
                  `exports.x` with `export`, and `__dirname` / `__filename` \
                  with `import.meta.dirname` / `import.meta.filename`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-text-content",
    description: "Prefer `.textContent` over `.innerText`.",
    remediation: "Replace `.innerText` with `.textContent`. \
                  `.textContent` is faster (no layout reflow), works on all \
                  node types, and returns text from hidden elements too.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "expression-complexity",
    description: "Overly complex expression with too many logical/conditional operators.",
    remediation: "Extract parts of the expression into named intermediate variables. Lines with 4+ logical/conditional operators are hard to read and reason about.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-dynamic-class",
    description: "Dynamic Tailwind classes are purged from the stylesheet.",
    remediation: "Use a static map instead of string interpolation: \
                  `const colors = { blue: 'bg-blue-500', red: 'bg-red-500' }; \
                  colors[color]`. Tailwind's purge only sees full static \
                  strings, so `bg-${color}-500` never ships.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["css", "tailwind"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-large-tuple-return",
    description: "Function return tuples with 3+ elements should be named structs.",
    remediation: "Replace `fn f() -> (A, B, C)` with `fn f() -> Result { … }` \
                  where `Result` is a named struct holding the same fields. \
                  Tuples force positional reasoning at every call site and \
                  make refactors impossible.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "number-literal-case",
    description: "Enforce proper case for numeric literals.",
    remediation: "Use lowercase for prefix/exponent (`0x`, `0b`, `0o`, `1e3`) and uppercase for hex digits (`0xFF`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "structured-api-error",
    description: "Bare `new Error()` in route handlers — use structured errors.",
    remediation: "Replace `new Error(\"message\")` with a structured error containing `{ type, code, status, detail }`. Bare Error messages are not machine-parseable and lack HTTP status context.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-hardcoded-secret",
    description: "Hardcoded secrets get exfiltrated from source control.",
    remediation: "Move the secret to an environment variable or secret \
                  store. Rotate the secret immediately — assume it is \
                  already compromised if it reached a commit.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-non-null-asserted-optional-chain",
    description: "Non-null assertion after optional chain contradicts its purpose.",
    remediation: "Remove the `!` — the optional chain already handles the nullish case.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-await-holding-lock",
    description: "Never hold a MutexGuard across an `.await` point.",
    remediation: "Drop the guard before awaiting: copy the needed data out \
                  in a tight scope, `drop(guard)`, then await. Locks held \
                  across awaits cause deadlocks under tokio's scheduler. \
                  Enable `clippy::await_holding_lock`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-deprecated-api",
    description: "Usage of deprecated Node.js or browser API.",
    remediation: "Replace with the modern equivalent: `Buffer.from()` instead of `new Buffer()`, `url.URL` instead of `url.parse()`, etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-weak-cipher",
    description: "Weak ciphers (DES, RC4, RC2, Blowfish) are cryptographically broken.",
    remediation: "Use AES-256-GCM or ChaCha20-Poly1305 instead of legacy ciphers.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-number-properties",
    description: "Prefer `Number.isNaN()`, `Number.parseInt()`, etc. over global equivalents.",
    remediation: "Replace global `isNaN()`, `isFinite()`, `parseInt()`, `parseFloat()`, `NaN`, \
                  and `Infinity` with their `Number.*` equivalents. The `Number` methods are \
                  stricter (no implicit coercion) and the properties are unambiguous.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-duplicate-enum-values",
    description: "Duplicate enum member values cause silent shadowing at runtime.",
    remediation: "Assign unique values to each enum member.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "max-function-lines",
    description: "Functions longer than 30 lines mix abstraction levels.",
    remediation: "Function exceeds 30 lines. Extract a named helper for the \
                  tail of the body — one level of abstraction per function.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
    crate::register_ts_family_with_clippy_marker!(META, typescript, "clippy::too_many_lines")
--
pub const META: RuleMeta = RuleMeta {
    id: "no-instanceof-builtins",
    description: "Avoid `instanceof` for built-in types — it fails across realms.",
    remediation: "Use `Array.isArray(x)` instead of `x instanceof Array`. \
                  For errors, check the `name` property or use `Error.isError()`. \
                  `instanceof` breaks across iframes, VMs, and module boundaries.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-lazy",
    description: "Lazy quantifier has no effect when the quantified token can only match a single length.",
    remediation: "Remove the `?` after the quantifier — it has no effect here.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-bidi-characters",
    description: "Invisible Unicode bidi control characters can be used in trojan-source attacks to disguise malicious code.",
    remediation: "Remove the bidi control character. If directional formatting is needed, use visible markup instead.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-invalid-fetch-options",
    description: "`fetch()` / `new Request()` with `body` on a GET or HEAD request is invalid.",
    remediation: "Remove the `body` property or change the method to POST/PUT/PATCH.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "hono-cookie-no-secure",
    description: "Cookie set without `secure` — sent over unencrypted HTTP.",
    remediation: "Add `secure: true` to cookie options so the cookie is only sent over HTTPS.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "hono"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-missing-example",
    description: "Exported function JSDoc must include an @example block.",
    remediation: "Add an `@example` block under the description showing a real \
                  call AND its return value: `@example\\n  const r = foo(42);\\n  // => 'forty-two'`. \
                  Examples are the fastest way for callers to understand the API.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "jsdoc"],
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-default-param-last",
    description: "Default parameters should be last to allow callers to omit them positionally.",
    remediation: "Move parameters with default values to the end of the parameter list.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/default-param-last/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-switch-over-chained-if",
    description: "Long if/else-if chains should be switch statements.",
    remediation: "Convert a 4+ branch if/else-if chain into a `switch` \
                  statement. Switch makes the discriminant obvious and \
                  lets TypeScript warn on missing cases for union types.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-deprecated-api",
    description: "Usage of deprecated Node.js or browser API.",
    remediation: "Replace with the modern equivalent: `Buffer.from()` instead of `new Buffer()`, `url.URL` instead of `url.parse()`, etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-next-description",
    description: "The `@next` tag in generator JSDoc must include a description.",
    remediation: "Add a description to the `@next` tag explaining the value passed to `next()`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-next-description.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-conflicting-classes",
    description: "Mutually exclusive Tailwind classes produce unpredictable styles.",
    remediation: "Keep only the intended utility. For example, `p-4 p-6` — \
                  remove one of the two padding values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-void-dom-elements-no-children",
    description: "Void HTML elements like `<br>`, `<img>`, `<input>` cannot have children.",
    remediation: "Remove children or `children`/`dangerouslySetInnerHTML` props \
                  from void elements. These elements are self-closing by spec — \
                  `<br />`, `<img />`, etc.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-to-contain",
    description: "Use `toContain()` instead of `expect(arr.includes(x)).toBe(true)`.",
    remediation: "Replace `expect(arr.includes(x)).toBe(true)` with \
                  `expect(arr).toContain(x)`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-to-contain.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-try-statements",
    description: "`try` blocks obscure error flow — prefer Result types or explicit error handling.",
    remediation: "Use a Result/Either type, or a wrapper function that returns `{ data, error }` tuples instead of try/catch.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-lazy",
    description: "Lazy quantifier has no effect when the quantified token can only match a single length.",
    remediation: "Remove the `?` after the quantifier — it has no effect here.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-while",
    description: "`for (;;)` or `for (;cond;)` without init/update — use `while` instead.",
    remediation: "Replace `for (;;)` with `while (true)` and `for (;condition;)` with `while (condition)`. The `for` form hides intent when init and update are unused.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-lonely-if",
    description: "Unexpected `if` as the only statement in an `else` block.",
    remediation: "Replace `else { if (cond) { ... } }` with `else if (cond) { ... }`. \
                  The `else if` form reduces nesting and makes the intent clearer. \
                  NOTE: this is different from `no-collapsible-if` which merges \
                  nested `if` without `else` using `&&`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-impl-debug-on-public-types",
    description: "Public structs and enums must derive `Debug`.",
    remediation: "Add `#[derive(Debug)]` (or `#[derive(Debug, …)]`) above \
                  the type definition. Every public type should be loggable \
                  for free, and consumers shouldn't have to wrap your type \
                  to get a `Debug` impl. If a field can't be Debug (e.g. a \
                  closure), implement `Debug` by hand instead.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-throws",
    description: "Functions that throw must document exceptions with `@throws`.",
    remediation: "Add a `@throws` tag documenting each exception the function may throw.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-throws.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-lonely-if",
    description: "Unexpected `if` as the only statement in an `else` block.",
    remediation: "Replace `else { if (cond) { ... } }` with `else if (cond) { ... }`. \
                  The `else if` form reduces nesting and makes the intent clearer. \
                  NOTE: this is different from `no-collapsible-if` which merges \
                  nested `if` without `else` using `&&`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-includes",
    description: "Prefer `.includes(x)` over `.indexOf(x) !== -1`.",
    remediation: "Replace `.indexOf(x) !== -1` or `.indexOf(x) >= 0` with `.includes(x)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-try-statements",
    description: "`try` blocks obscure error flow — prefer Result types or explicit error handling.",
    remediation: "Use a Result/Either type, or a wrapper function that returns `{ data, error }` tuples instead of try/catch.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-duplicate-enum-values",
    description: "Duplicate enum member values cause silent shadowing at runtime.",
    remediation: "Assign unique values to each enum member.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-bool-return-from-fallible",
    description: "Action functions return `Result`, not `bool`.",
    remediation: "Change the return type to `Result<T, E>` (use `()` for \
                  T if there's no payload). A bool tells the caller \
                  something failed but not why — they can't handle the \
                  error specifically, only choose to give up or retry blindly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-useless-fragment",
    description: "Unnecessary `<Fragment>` that wraps a single child or nothing.",
    remediation: "Remove the fragment wrapper when it contains only one child or \
                  is empty. Fragments are only needed to group multiple siblings.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/jsx-no-useless-fragment.md"),
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-multi-op-oneliner",
    description: "Dense one-liners with many chained operators resist review.",
    remediation: "Extract intermediate named variables. Each step of the \
                  expression should have a name that says what it represents \
                  — `activeItems`, `prices`, `subtotal`, `total`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
--
pub const META: RuleMeta = RuleMeta {
    id: "react-refresh-only-export-components",
    description: "Non-component exports alongside component exports break React Fast Refresh (HMR).",
    remediation: "Move non-component exports (constants, utilities, types) to a separate module. Only export React components from files that also export components, so HMR can update them without a full reload.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-prefer-char-class",
    description: "Single-character alternations should use a character class.",
    remediation: "Replace `a|b|c` with `[abc]`. Character classes are more readable and often faster than alternation for single characters.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-trivially-nested-assertion",
    description: "Lookaround assertion is trivially nested inside another lookaround of the same kind.",
    remediation: "Merge the nested lookaround into its parent or simplify the structure.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-trivially-nested-assertion.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-type-error",
    description: "Use `TypeError` instead of `Error` in type-checking conditions.",
    remediation: "When throwing inside an `if` that performs a type check \
                  (typeof, instanceof, Array.isArray, etc.), use `new TypeError()` \
                  instead of `new Error()` to signal the caller passed a wrong type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-skipped-test",
    description: "Skipped tests silently erode coverage.",
    remediation: "Remove the `.skip()` annotation, fix the test, or delete it \
                  if it's no longer relevant.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-skipped-test.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-large-enum-variant",
    description: "Enum size equals the largest variant — box big variants.",
    remediation: "Wrap the large variant's payload in `Box<T>` so the enum \
                  stays small. Otherwise every instance of the enum — even \
                  the small-variant case — pays the full size cost. Enable \
                  `clippy::large_enum_variant`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-empty-function",
    description: "Empty functions are often a sign of incomplete refactoring.",
    remediation: "Add a comment explaining why the function is intentionally empty, or remove it.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-empty-function/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-duplicate-string",
    description: "String literal appears 3+ times — extract to a constant.",
    remediation: "Extract the repeated string into a named constant and reference it everywhere. This reduces typo risk and makes future changes a single-line edit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-duplicate-string",
    description: "String literal appears 3+ times — extract to a constant.",
    remediation: "Extract the repeated string into a named constant and reference it everywhere. This reduces typo risk and makes future changes a single-line edit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-primitive-wrappers",
    description: "`new String()`, `new Number()`, `new Boolean()` create wrapper objects, not primitives.",
    remediation: "Use primitive literals or factory functions without `new`: `String(x)`, `Number(x)`, `Boolean(x)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "function-return-type",
    description: "Functions should not return literals of different types.",
    remediation: "Ensure all return statements in a function return the same type of literal, or use a discriminated union type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-invariant-returns",
    description: "Function always returns the same literal value.",
    remediation: "If the return value never varies, the function likely has dead logic or should be a constant.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-dynamic-delete",
    description: "Using `delete` on a computed key is error-prone — use `Map` or `Set` instead.",
    remediation: "Remove the dynamic `delete` and use a `Map`/`Set`, or delete a static key.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-dynamic-delete/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-ignored-exceptions",
    description: "Empty `catch` block silently swallows exceptions.",
    remediation: "At minimum, log the error or re-throw it. Silent catch blocks hide bugs and make debugging extremely difficult. If intentional, add an explanatory comment.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "migration-needs-lock-timeout",
    description: "DDL migration without `SET lock_timeout` risks write queue pileups.",
    remediation: "Add `SET lock_timeout = '5s';` at the top of every DDL migration. Without it, an ALTER TABLE on a busy table queues all writes behind the lock indefinitely.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "migrations"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-includes",
    description: "Prefer `.includes(x)` over `.indexOf(x) !== -1`.",
    remediation: "Replace `.indexOf(x) !== -1` or `.indexOf(x) >= 0` with `.includes(x)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-hook-setter-in-body",
    description: "`useState` setter called directly in component body causes infinite re-renders.",
    remediation: "Move the setter call inside `useEffect`, `useCallback`, or an event handler.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-nullish-default-on-input",
    description: "Defaulting function parameters silently paves over invalid input.",
    remediation: "Don't use `??` or `||` to default a function parameter. \
                  Validate at the boundary: if the input is invalid, return \
                  a Result error. Silent defaults turn caller bugs into \
                  silent wrong answers.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-iterator-to-array",
    description: "Disallow unnecessary `.toArray()` on iterators.",
    remediation: "Remove `.toArray()` — the consuming context already accepts \
                  iterables. `for…of`, spread, `yield*`, `new Set(…)`, \
                  `Array.from(…)`, and `Object.fromEntries(…)` all work \
                  directly on iterators.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-prefer-set-operation",
    description: "Lookaround combined with a character can be expressed more clearly using a set operation.",
    remediation: "Replace the lookaround pattern with a `v`-flag character class set operation.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/prefer-set-operation.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-large-enum-variant",
    description: "Enum size equals the largest variant — box big variants.",
    remediation: "Wrap the large variant's payload in `Box<T>` so the enum \
                  stays small. Otherwise every instance of the enum — even \
                  the small-variant case — pays the full size cost. Enable \
                  `clippy::large_enum_variant`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-backreference",
    description: "Backreference is always replaced by the empty string because it references itself or a group that has not yet been matched.",
    remediation: "Remove the useless backreference or restructure the regex so the referenced group is matched before the backreference.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-useless-backreference.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-regexp-exec",
    description: "`.match(/regex/)` is slower than `regex.exec(string)` for non-global regexps.",
    remediation: "Use `regex.exec(string)` instead of `string.match(regex)`. For non-global regular expressions, `RegExp.prototype.exec()` is faster and returns the same result.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-sort-tags",
    description: "JSDoc tags must follow canonical order: `@param` before `@returns` before `@throws` before `@example`.",
    remediation: "Reorder the tags: `@param` first, then `@returns`, then `@throws`, then `@example`. Consistent ordering makes JSDoc blocks scannable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-absolute-path",
    description: "Import uses an absolute path — use relative or aliased paths.",
    remediation: "Replace the absolute path import with a relative path (`./…`) or a configured path alias (`@/…`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-checked-requires-onchange",
    description: "`checked` prop without `onChange` or `readOnly` makes the input uncontrollable.",
    remediation: "Add an `onChange` handler or `readOnly` prop. Without either, \
                  React renders a frozen checkbox/radio that the user cannot \
                  interact with, and emits a console warning.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-group",
    description: "Empty capturing group `()` is likely a mistake.",
    remediation: "Remove the empty group or add a pattern inside it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "function-return-type",
    description: "Functions should not return literals of different types.",
    remediation: "Ensure all return statements in a function return the same type of literal, or use a discriminated union type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-react-setstate",
    description: "Calling a `useState` setter with its own state value is a no-op.",
    remediation: "Remove the useless `setState` call or pass a different value. `setX(x)` triggers a re-render but does not change state.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-useless-constructor",
    description: "Empty constructors that only call `super()` are unnecessary.",
    remediation: "Remove the constructor — the default behaviour is identical.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-useless-constructor/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-inconsistent-returns",
    description: "Function has inconsistent returns — some paths return a value, others return nothing.",
    remediation: "Ensure every return path either returns a value or returns nothing. Mixing `return expr;` with bare `return;` or implicit returns is confusing.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-unwrap",
    description: "No `.unwrap()` / `.expect()` in production code.",
    remediation: "Handle the None / Err case explicitly. Use `?` with \
                  proper error propagation, or `unwrap_or_else` with a \
                  meaningful fallback. `unwrap()` turns runtime conditions \
                  into crashes. Tests are exempted — panicking inside a \
                  `#[test]` is a clean failure.",
    severity: Severity::Error,
    doc_url: None,
--

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-alternative",
    description: "Empty alternative in regex matches empty string and is likely a mistake.",
    remediation: "Remove the leading, trailing, or consecutive `|` in the regex pattern.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "comment-prose-quality",
    description: "Comments with weasel words, passive voice, or lexical illusions \
                  reduce clarity.",
    remediation: "Rewrite the comment to be direct. Replace passive voice with \
                  active. Remove filler words. Fix repeated words.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "number-literal-case",
    description: "Enforce proper case for numeric literals.",
    remediation: "Use lowercase for prefix/exponent (`0x`, `0b`, `0o`, `1e3`) and uppercase for hex digits (`0xFF`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "vue-v-for-needs-stable-key",
    description: "v-for `:key` must use a stable identifier, not the loop index.",
    remediation: "Replace `:key=\"index\"` / `:key=\"i\"` with a stable id from \
                  the data: `:key=\"item.id\"`. Index keys cause Vue to reuse the \
                  wrong DOM when items reorder, filter, or get inserted.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "vue"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-explicit-iter-loop",
    description: "Use iterator chains, not raw index loops.",
    remediation: "Replace `for i in 0..vec.len() { vec[i] }` with \
                  `for x in &vec`. Iterator chains let the compiler \
                  vectorize the loop body and eliminate bounds checks. \
                  Enable `clippy::needless_range_loop` and \
                  `clippy::explicit_iter_loop`.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-param-name",
    description: "Every `@param` tag must include a parameter name.",
    remediation: "Add the parameter name after the type annotation — `@param {string} name` instead of `@param {string}`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-param-name.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-multiple-spaces",
    description: "Multiple consecutive spaces in regex are hard to read and count.",
    remediation: "Use a quantifier: ` {2}` or `\\s{2,}` instead of multiple spaces.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unnecessary-await",
    description: "Do not `await` non-promise values.",
    remediation: "Remove the unnecessary `await` — literals, arrays, functions, \
                  and other non-thenable values resolve synchronously and the \
                  `await` just adds confusion.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-default-params",
    description: "Default parameters hide behavior and create invisible coupling.",
    remediation: "Replace default parameters with explicit factory methods. \
                  `createUser(name, role = 'viewer')` → `createViewer(name)` \
                  and `createAdmin(name)`. Each factory is self-documenting \
                  and independently testable.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-and-in-function-name",
    description: "`And` in a function name signals two responsibilities — split it.",
    remediation: "A function with `And` in its name does two things. Split into \
                  two functions named after each responsibility, then let the caller \
                  compose them: `getUserAndUpdateCache` → `getUser()` + `updateCache(user)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["naming"],
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-informative-docs",
    description: "JSDoc description merely repeats the name of the symbol without adding useful information.",
    remediation: "Rewrite the JSDoc to explain *why* or *how* the symbol works, not just restate its name.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/informative-docs.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-children-prop",
    description: "Passing `children` as a prop instead of nesting content.",
    remediation: "Place children between the opening and closing tags instead of \
                  passing them as a `children` prop. This is more readable and \
                  idiomatic.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-children-prop.md"),
    categories: &["react"],
};
--
--
pub const META: RuleMeta = RuleMeta {
    id: "vue-v-for-needs-stable-key",
    description: "v-for `:key` must use a stable identifier, not the loop index.",
    remediation: "Replace `:key=\"index\"` / `:key=\"i\"` with a stable id from \
                  the data: `:key=\"item.id\"`. Index keys cause Vue to reuse the \
                  wrong DOM when items reorder, filter, or get inserted.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "vue"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "import-dynamic-import-chunkname",
    description: "Dynamic imports require a leading `webpackChunkName` comment.",
    remediation: "Add a `/* webpackChunkName: \"name\" */` comment before the import source.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/dynamic-import-chunkname.md"),
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-nested-assignment",
    description: "Assignment inside a condition or sub-expression is likely a bug.",
    remediation: "Move the assignment before the condition: `x = value; if (x) { ... }`. If intentional, use a separate statement.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-magic-array-flat-depth",
    description: "Disallow a magic number as the `depth` argument in `Array#flat()`.",
    remediation: "Extract the depth into a named constant, or use `Infinity` for unbounded flattening.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "require-module-specifiers",
    description: "Import/export statements with empty specifier lists are not allowed.",
    remediation: "Add specifiers to the import/export, convert to a side-effect \
                  import, or remove the statement entirely.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-null",
    description: "Use `undefined` instead of `null`.",
    remediation: "Replace `null` with `undefined`. Having two nullish values \
                  in the language is a footgun — standardize on `undefined` to \
                  reduce null-check surface area.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-await-holding-lock",
    description: "Never hold a MutexGuard across an `.await` point.",
    remediation: "Drop the guard before awaiting: copy the needed data out \
                  in a tight scope, `drop(guard)`, then await. Locks held \
                  across awaits cause deadlocks under tokio's scheduler. \
                  Enable `clippy::await_holding_lock`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "explicit-return-type-on-exported",
    description: "Exported functions must declare their return type.",
    remediation: "Add an explicit `: ReturnType` annotation after the \
                  parameters of every exported function. This locks the \
                  public contract and prevents silent drift when the \
                  implementation changes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-extraneous-class",
    description: "Classes with only static members or an empty body should be plain objects or modules.",
    remediation: "Use a module/namespace, plain object, or standalone functions instead.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-extraneous-class/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unnecessary-array-splice-count",
    description: "Disallow unnecessary `.length` or `Infinity` as the count argument of `Array#splice()` / `Array#toSpliced()`.",
    remediation: "Remove the second argument: `.splice(start)` deletes all elements from `start` to the end.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
--
pub const META: RuleMeta = RuleMeta {
    id: "no-raw-db-entity-in-handler",
    description: "Route handlers should not return raw DB queries directly.",
    remediation: "Map the DB entity to a DTO before returning from the route handler. Returning raw DB entities leaks internal schema details, couples the API shape to the database, and makes it easy to accidentally expose sensitive columns.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-jump",
    description: "Redundant `return;` at end of function or `continue;` at end of loop body.",
    remediation:
        "Remove the redundant `return;` or `continue;` — execution already falls through naturally.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-class-fields",
    description: "Prefer class field declarations over `this` assignments in constructors for static values.",
    remediation: "Move the literal assignment from the constructor to a class \
                  field declaration. Class fields are more declarative and \
                  make the default value visible at a glance.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-promise-shorthand",
    description: "`new Promise` wrapping a single `resolve`/`reject` call — use `Promise.resolve`/`Promise.reject` instead.",
    remediation: "Replace `new Promise((resolve) => resolve(x))` with `Promise.resolve(x)` and `new Promise((_, reject) => reject(x))` with `Promise.reject(x)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-module",
    description: "Prefer ESM (`import`/`export`) over CommonJS (`require`/`module.exports`).",
    remediation: "Replace `require()` with `import`, `module.exports` / \
                  `exports.x` with `export`, and `__dirname` / `__filename` \
                  with `import.meta.dirname` / `import.meta.filename`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-pg-enum",
    description: "PostgreSQL `CREATE TYPE ... AS ENUM` — can't remove values once added.",
    remediation: "Replace PG enums with a CHECK constraint (`status TEXT CHECK(status IN ('a','b','c'))`) or a lookup table. PG enums can't have values removed — they're append-only, which makes rollbacks impossible.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-xml-external-entity",
    description: "XML parsers without XXE protection are vulnerable to external entity attacks.",
    remediation: "Disable external entities: set `noent: false` or `externalEntities: false` when configuring XML parsers.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--

--

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-useless-not",
    description: "Using `.not.toBeVisible()` when `.toBeHidden()` exists is needlessly indirect.",
    remediation: "Use the direct matcher instead of negating: \
                  `toBeHidden` instead of `not.toBeVisible`, \
                  `toBeDisabled` instead of `not.toBeEnabled`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-useless-not.md"),
    categories: &["testing"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-explicit-function-return-type",
    description: "Functions and class methods should have explicit return types for documentation and safety.",
    remediation: "Add an explicit return type annotation to the function.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/explicit-function-return-type/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "require-module-specifiers",
    description: "Import/export statements with empty specifier lists are not allowed.",
    remediation: "Add specifiers to the import/export, convert to a side-effect \
                  import, or remove the statement entirely.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-type-over-interface",
    description: "Prefer `type` over `interface` unless you need extension.",
    remediation: "Replace `interface X { ... }` with `type X = { ... }`. \
                  Types support unions, intersections, mapped types, and \
                  conditional types. Keep `interface` only when you need \
                  `extends` or declaration merging.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
--
pub const META: RuleMeta = RuleMeta {
    id: "node-no-callback-literal",
    description: "First argument to error-first callbacks should be an Error object or `null`, not a string literal.",
    remediation: "Pass `new Error('...')` or `null` as the first argument instead of a string literal.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unnecessary-array-splice-count",
    description: "Disallow unnecessary `.length` or `Infinity` as the count argument of `Array#splice()` / `Array#toSpliced()`.",
    remediation: "Remove the second argument: `.splice(start)` deletes all elements from `start` to the end.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "max-file-lines",
    description: "Files longer than 200 lines carry too many responsibilities.",
    remediation: "File exceeds 200 lines. Split by responsibility — extract \
                  helpers into a separate module.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "banned-comment-words",
    description: "Dismissive filler words in comments hide complexity instead of explaining it.",
    remediation: "Remove the filler word and rewrite the comment to explain the actual \
                  subtlety. If the line is genuinely obvious, delete the comment instead. \
                  Banned: obviously, simply, just, basically, clearly, trivially.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["comments"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "structured-api-error",
    description: "Bare `new Error()` in route handlers — use structured errors.",
    remediation: "Replace `new Error(\"message\")` with a structured error containing `{ type, code, status, detail }`. Bare Error messages are not machine-parseable and lack HTTP status context.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-anonymous-default-export",
    description: "Disallow anonymous functions and classes as the default export.",
    remediation: "Name the exported function or class. Anonymous default \
                  exports break refactoring tools, produce unhelpful stack \
                  traces, and make `import` auto-complete less useful.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-super-linear-move",
    description: "Regex quantifier can cause quadratic runtime on certain inputs.",
    remediation: "Refactor the quantifier to avoid super-linear backtracking. Use atomic groups or restructure the pattern.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-super-linear-move.html"),
    categories: &["security", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-type-over-interface",
    description: "Prefer `type` over `interface` unless you need extension.",
    remediation: "Replace `interface X { ... }` with `type X = { ... }`. \
                  Types support unions, intersections, mapped types, and \
                  conditional types. Keep `interface` only when you need \
                  `extends` or declaration merging.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-class-fields",
    description: "Prefer class field declarations over `this` assignments in constructors for static values.",
    remediation: "Move the literal assignment from the constructor to a class \
                  field declaration. Class fields are more declarative and \
                  make the default value visible at a glance.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-octal",
    description: "Octal escapes in regex (`\\1`, `\\12`) are ambiguous — they could be backreferences or octal character codes.",
    remediation: "Use named backreferences (`\\k<name>`) or explicit Unicode escapes (`\\u{...}`) instead of bare octal sequences.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-set-size",
    description: "Prefer `Set#size` instead of spreading into an array and reading `.length`.",
    remediation: "Replace `[...mySet].length` or `Array.from(mySet).length` \
                  with `mySet.size`. Spreading a Set into an array just to \
                  read its length is wasteful — `Set#size` is O(1).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--

--
pub const META: RuleMeta = RuleMeta {
    id: "data-clumps",
    description: "Same 3+ parameter names appear together in multiple function signatures.",
    remediation: "Extract the repeated parameter group into a value object / options type. Data clumps indicate a missing abstraction — e.g. `(host, port, protocol)` should be a `ConnectionConfig`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-incorrect-string-concat",
    description: "Suspicious string concatenation with a number variable.",
    remediation: "Use explicit conversion: `\"text\" + String(num)` or template literals: `\\`text${num}\\``.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "zod-no-any",
    description: "`z.any()` disables validation and type narrowing.",
    remediation: "Replace `z.any()` with `z.unknown()`. The runtime \
                  behavior is the same (everything accepted) but the \
                  TypeScript type is `unknown`, forcing downstream code \
                  to narrow before using the value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "zod"],
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-init-declarations",
    description: "Variables should be initialized at declaration — uninitialized declarations are error-prone.",
    remediation: "Add an initializer to the variable declaration, or use `declare` for ambient contexts.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/init-declarations"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-missing-example",
    description: "Exported function JSDoc must include an @example block.",
    remediation: "Add an `@example` block under the description showing a real \
                  call AND its return value: `@example\\n  const r = foo(42);\\n  // => 'forty-two'`. \
                  Examples are the fastest way for callers to understand the API.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "jsdoc"],
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-anchor-precedence",
    description: "Anchor `^` or `$` in alternation may not bind as expected.",
    remediation: "Wrap the alternation in a group: `/^(a|b)$/` instead of `/^a|b$/`. Without grouping, `/^a|b$/` means `(^a)|(b$)`, not `^(a|b)$`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-new-regex-with-variable",
    description: "`new RegExp(variable)` enables ReDoS attacks.",
    remediation: "Replace dynamic regex construction with a literal regex \
                  or a vetted safe-regex library. User-controlled patterns \
                  can trigger exponential backtracking and freeze the event loop.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-octal",
    description: "Octal escapes in regex (`\\1`, `\\12`) are ambiguous — they could be backreferences or octal character codes.",
    remediation: "Use named backreferences (`\\k<name>`) or explicit Unicode escapes (`\\u{...}`) instead of bare octal sequences.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-new-regex-with-variable",
    description: "`new RegExp(variable)` enables ReDoS attacks.",
    remediation: "Replace dynamic regex construction with a literal regex \
                  or a vetted safe-regex library. User-controlled patterns \
                  can trigger exponential backtracking and freeze the event loop.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
--
pub const META: RuleMeta = RuleMeta {
    id: "react-refresh-only-export-components",
    description: "Non-component exports alongside component exports break React Fast Refresh (HMR).",
    remediation: "Move non-component exports (constants, utilities, types) to a separate module. Only export React components from files that also export components, so HMR can update them without a full reload.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-mod-tests-without-cfg-test",
    description: "`mod tests` must be gated by `#[cfg(test)]`.",
    remediation: "Add `#[cfg(test)]` immediately above the `mod tests` \
                  declaration. Without it, every test function ships in \
                  the release binary — bloat plus a risk of pulling in \
                  dev-dependencies that aren't built for release.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-logger-in-business-logic",
    description: "Logging calls in business logic (service/domain/core/model/entity layers).",
    remediation: "Remove direct `logger.*` / `console.log` calls from business logic. Use a `withLogging()` wrapper or emit domain events instead. Logging is a cross-cutting concern — it belongs in infrastructure, not domain code.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-explicit-member-accessibility",
    description: "Class properties and methods should have explicit accessibility modifiers.",
    remediation: "Add `public`, `private`, or `protected` to the class member declaration.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/explicit-member-accessibility/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-and-in-function-name",
    description: "`And` in a function name signals two responsibilities — split it.",
    remediation: "A function with `And` in its name does two things. Split into \
                  two functions named after each responsibility, then let the caller \
                  compose them: `getUserAndUpdateCache` → `getUser()` + `updateCache(user)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["naming"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-logger-in-business-logic",
    description: "Logging calls in business logic (service/domain/core/model/entity layers).",
    remediation: "Remove direct `logger.*` / `console.log` calls from business logic. Use a `withLogging()` wrapper or emit domain events instead. Logging is a cross-cutting concern — it belongs in infrastructure, not domain code.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-iterator-to-array",
    description: "Disallow unnecessary `.toArray()` on iterators.",
    remediation: "Remove `.toArray()` — the consuming context already accepts \
                  iterables. `for…of`, spread, `yield*`, `new Set(…)`, \
                  `Array.from(…)`, and `Object.fromEntries(…)` all work \
                  directly on iterators.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-page-click-deprecated",
    description: "`page.click(selector)` is deprecated — use `page.locator(selector).click()`.",
    remediation: "Replace `page.click(selector)` with \
                  `page.locator(selector).click()`. The locator API \
                  auto-waits and auto-retries, and the direct page methods \
                  are deprecated in Playwright.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-primitive-wrappers",
    description: "`new String()`, `new Number()`, `new Boolean()` create wrapper objects, not primitives.",
    remediation: "Use primitive literals or factory functions without `new`: `String(x)`, `Number(x)`, `Boolean(x)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "comment-prose-quality",
    description: "Comments with weasel words, passive voice, or lexical illusions \
                  reduce clarity.",
    remediation: "Rewrite the comment to be direct. Replace passive voice with \
                  active. Remove filler words. Fix repeated words.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-nullish-default-on-input",
    description: "Defaulting function parameters silently paves over invalid input.",
    remediation: "Don't use `??` or `||` to default a function parameter. \
                  Validate at the boundary: if the input is invalid, return \
                  a Result error. Silent defaults turn caller bugs into \
                  silent wrong answers.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-test-logic",
    description: "Tests with `if`/`for`/`while`/`switch` are testing the test, not the code.",
    remediation: "Remove control-flow logic from test bodies. Use \
                  `test.each()` for data-driven tests, extract shared \
                  setup to `beforeEach`, and write one assertion path per \
                  test. Logic in tests hides which branch actually ran, \
                  making failures hard to diagnose.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-triple-slash-reference",
    description: "Triple-slash reference directives are legacy — use ES `import` instead.",
    remediation: "Replace `/// <reference path=\"...\" />` or `/// <reference types=\"...\" />` with an ES `import` declaration.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/triple-slash-reference"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-flat",
    description: "Prefer `.flat()` over legacy array flattening techniques.",
    remediation: "Replace `[].concat(…arr)` or `.reduce((a,b) => a.concat(b), [])` with `.flat()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-default-params",
    description: "Default parameters hide behavior and create invisible coupling.",
    remediation: "Replace default parameters with explicit factory methods. \
                  `createUser(name, role = 'viewer')` → `createViewer(name)` \
                  and `createAdmin(name)`. Each factory is self-documenting \
                  and independently testable.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
--
};

--
pub const META: RuleMeta = RuleMeta {
    id: "max-dependencies",
    description: "File has too many import dependencies — consider splitting.",
    remediation: "Reduce the number of imports by extracting logic into sub-modules or by re-evaluating whether all dependencies are necessary.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-invariant-returns",
    description: "Function always returns the same literal value.",
    remediation: "If the return value never varies, the function likely has dead logic or should be a constant.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "max-dependencies",
    description: "File has too many import dependencies — consider splitting.",
    remediation: "Reduce the number of imports by extracting logic into sub-modules or by re-evaluating whether all dependencies are necessary.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-unwrap",
    description: "No `.unwrap()` / `.expect()` in production code.",
    remediation: "Handle the None / Err case explicitly. Use `?` with \
                  proper error propagation, or `unwrap_or_else` with a \
                  meaningful fallback. `unwrap()` turns runtime conditions \
                  into crashes. Tests are exempted — panicking inside a \
                  `#[test]` is a clean failure.",
    severity: Severity::Error,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-assertions",
    description: "Regex contains an assertion that is always true or always false, making it useless.",
    remediation: "Remove the useless assertion or restructure the pattern so the assertion is meaningful.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-useless-assertions.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-null",
    description: "Use `undefined` instead of `null`.",
    remediation: "Replace `null` with `undefined`. Having two nullish values \
                  in the language is a footgun — standardize on `undefined` to \
                  reduce null-check surface area.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "db-no-n-plus-one",
    description: "`await db.query` inside a loop is an N+1 query — use a JOIN or batch query.",
    remediation: "Move the query outside the loop: use a JOIN, `WHERE id IN (...)`, or batch fetch. N+1 queries scale linearly with result set size and are the #1 cause of slow pages.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-script-url",
    description: "`href=\"javascript:...\"` is an XSS vector.",
    remediation: "Use an `onClick` handler instead of a `javascript:` URL. \
                  Script URLs bypass CSP and enable cross-site scripting.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-ptr-arg",
    description: "Prefer borrowed slices over borrowed owned types.",
    remediation: "Replace `&String` with `&str`, `&Vec<T>` with `&[T]`, \
                  `&PathBuf` with `&Path`. The slice form accepts more \
                  caller types with no extra cost. Enable `clippy::ptr_arg`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "db-no-n-plus-one",
    description: "`await db.query` inside a loop is an N+1 query — use a JOIN or batch query.",
    remediation: "Move the query outside the loop: use a JOIN, `WHERE id IN (...)`, or batch fetch. N+1 queries scale linearly with result set size and are the #1 cause of slow pages.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-explicit-iter-loop",
    description: "Use iterator chains, not raw index loops.",
    remediation: "Replace `for i in 0..vec.len() { vec[i] }` with \
                  `for x in &vec`. Iterator chains let the compiler \
                  vectorize the loop body and eliminate bounds checks. \
                  Enable `clippy::needless_range_loop` and \
                  `clippy::explicit_iter_loop`.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-ptr-arg",
    description: "Prefer borrowed slices over borrowed owned types.",
    remediation: "Replace `&String` with `&str`, `&Vec<T>` with `&[T]`, \
                  `&PathBuf` with `&Path`. The slice form accepts more \
                  caller types with no extra cost. Enable `clippy::ptr_arg`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-await-expression-member",
    description: "Do not access a member directly from an await expression.",
    remediation: "Extract the awaited value into a variable, then access the member: \
                  `const response = await fetch(url); const data = response.json();`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-magic-array-flat-depth",
    description: "Disallow a magic number as the `depth` argument in `Array#flat()`.",
    remediation: "Extract the depth into a named constant, or use `Infinity` for unbounded flattening.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-dupe-disjunctions",
    description: "Regex contains duplicate alternatives that are redundant.",
    remediation: "Remove the duplicate alternative from the disjunction.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-dupe-disjunctions.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "group-exports",
    description: "Multiple named export declarations — consolidate into a single export block.",
    remediation: "Gather all named exports into a single `export { … }` declaration at the bottom of the file instead of scattering `export` across multiple declarations.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-raw-db-entity-in-handler",
    description: "Route handlers should not return raw DB queries directly.",
    remediation: "Map the DB entity to a DTO before returning from the route handler. Returning raw DB entities leaks internal schema details, couples the API shape to the database, and makes it easy to accidentally expose sensitive columns.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "require-post-message-target-origin",
    description: "`postMessage()` called without the `targetOrigin` argument.",
    remediation: "Always provide a `targetOrigin` argument (e.g. `self.location.origin` or `'*'`) to `postMessage()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-for-of",
    description: "A `for` loop whose index is only used for array access can be a simpler `for-of`.",
    remediation: "Replace the `for` loop with `for (const item of array)`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-for-of/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-negation-in-equality-check",
    description: "Negated expression in equality check is a precedence bug.",
    remediation: "`!x === y` is parsed as `(!x) === y`, not `!(x === y)`. \
                  Use `x !== y` or wrap explicitly: `!(x === y)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-this-alias",
    description: "Assigning `this` to a variable is a legacy pattern — use arrow functions instead.",
    remediation: "Use an arrow function to capture `this` lexically.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-this-alias/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-returns",
    description: "Functions with return statements must have `@returns` in their JSDoc.",
    remediation: "Add an `@returns` tag describing what the function returns. This helps callers understand the return value without reading the implementation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-returns",
    description: "Functions with return statements must have `@returns` in their JSDoc.",
    remediation: "Add an `@returns` tag describing what the function returns. This helps callers understand the return value without reading the implementation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-xml-external-entity",
    description: "XML parsers without XXE protection are vulnerable to external entity attacks.",
    remediation: "Disable external entities: set `noent: false` or `externalEntities: false` when configuring XML parsers.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-incorrect-string-concat",
    description: "Suspicious string concatenation with a number variable.",
    remediation: "Use explicit conversion: `\"text\" + String(num)` or template literals: `\\`text${num}\\``.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "explicit-return-type-on-exported",
    description: "Exported functions must declare their return type.",
    remediation: "Add an explicit `: ReturnType` annotation after the \
                  parameters of every exported function. This locks the \
                  public contract and prevents silent drift when the \
                  implementation changes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-absolute-path",
    description: "Import uses an absolute path — use relative or aliased paths.",
    remediation: "Replace the absolute path import with a relative path (`./…`) or a configured path alias (`@/…`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-constructed-context-values",
    description: "`<Provider value={{ ... }}>` creates a new object every render, causing all consumers to re-render.",
    remediation: "Memoize the context value with `useMemo` or extract it to a \
                  stable reference. `<Provider value={memoized}>` avoids \
                  unnecessary re-renders of every consumer.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};
--

--
pub const META: RuleMeta = RuleMeta {
    id: "cyclomatic-complexity",
    description: "Functions with cyclomatic complexity > 10 are hard to test and maintain.",
    remediation: "Refactor the function: extract helper functions, use early returns, replace conditionals with polymorphism or lookup tables.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-template-description",
    description: "Every `@template` tag must include a description.",
    remediation: "Add a description after the template name explaining the type parameter's purpose.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-template-description.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "de-morgan-simplify",
    description: "Apply De Morgan's law: `!(a && b)` is `!a || !b`, `!(a || b)` is `!a && !b`.",
    remediation: "Distribute the negation using De Morgan's law. `!(a && b)` becomes `!a || !b` and `!(a || b)` becomes `!a && !b`. The expanded form is easier to reason about.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-promise-shorthand",
    description: "`new Promise` wrapping a single `resolve`/`reject` call — use `Promise.resolve`/`Promise.reject` instead.",
    remediation: "Replace `new Promise((resolve) => resolve(x))` with `Promise.resolve(x)` and `new Promise((_, reject) => reject(x))` with `Promise.reject(x)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-misplaced-loop-counter",
    description: "`for` loop update clause modifies a different variable than the condition.",
    remediation: "Ensure the update expression (`i++`) modifies the same variable used in the loop condition (`i < n`). Mismatched variables usually indicate a copy-paste bug.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-unreadable-array-destructuring",
    description: "Array destructuring may not contain consecutive ignored values.",
    remediation: "Use index access instead: `const third = arr[2]`. \
                  Consecutive commas like `[,, x,,,, y]` are hard to read \
                  and easy to miscount.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-jump",
    description: "Redundant `return;` at end of function or `continue;` at end of loop body.",
    remediation:
        "Remove the redundant `return;` or `continue;` — execution already falls through naturally.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-identical-functions",
    description: "Two functions have identical implementations.",
    remediation: "Extract the duplicated logic into a shared helper. Identical functions diverge silently when one gets patched.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "group-exports",
    description: "Multiple named export declarations — consolidate into a single export block.",
    remediation: "Gather all named exports into a single `export { … }` declaration at the bottom of the file instead of scattering `export` across multiple declarations.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-keyboard-event-key",
    description: "Prefer `KeyboardEvent#key` over `KeyboardEvent#keyCode`.",
    remediation: "Use `event.key` instead of `event.keyCode`, `event.charCode`, or `event.which`. The `.key` property returns a human-readable string and is the modern standard.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "use-type-alias",
    description: "Repeated complex inline type annotations should be extracted into a type alias.",
    remediation: "Create a `type` alias for the repeated annotation and use it in all positions.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "import-no-empty-named-blocks",
    description: "Empty named import blocks are forbidden.",
    remediation: "Remove the empty `import { }` or add the intended named imports.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/no-empty-named-blocks.md"),
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-complexity",
    description: "Regex pattern is overly complex (score > 20).",
    remediation: "Break the regex into smaller named patterns or use a parser. Complex regex is hard to read, test, and maintain.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-dupe-class-members",
    description: "Duplicate class members shadow earlier definitions and indicate a bug.",
    remediation: "Remove or rename the duplicate class member. TS method overloads (without a body) are allowed.",
    severity: Severity::Error,
    doc_url: Some("https://typescript-eslint.io/rules/no-dupe-class-members"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-typos",
    description: "Probable typo in React component static property or lifecycle method.",
    remediation: "Fix the typo. Common mistakes include `getDerivedStateFromProp` \
                  (should be `getDerivedStateFromProps`) and `componentWillRecieveProps` \
                  (should be `componentWillReceiveProps`).",
    severity: Severity::Error,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-typos.md"),
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-trim-start-end",
    description: "Prefer `String#trimStart()` / `String#trimEnd()` over the deprecated `trimLeft()` / `trimRight()`.",
    remediation: "Replace `.trimLeft()` with `.trimStart()` and `.trimRight()` with `.trimEnd()`. \
                  The `trimLeft`/`trimRight` aliases are deprecated in favor of the spec names.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-empty-object-type",
    description: "`{}` as a type matches any non-nullish value — it almost never means what you think.",
    remediation: "Use `Record<string, never>` for an empty object, `object` for any object, \
                  or `unknown` for any value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
--
pub const META: RuleMeta = RuleMeta {
    id: "max-file-lines",
    description: "Files longer than 200 lines carry too many responsibilities.",
    remediation: "File exceeds 200 lines. Split by responsibility — extract \
                  helpers into a separate module.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-unstable-nested-components",
    description: "Component defined inside another component causes unmount/remount every render.",
    remediation: "Move the inner component outside the parent component. Defining a \
                  component inside render means React sees a brand-new type on every \
                  render, destroying the entire subtree's DOM nodes and state.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "node-no-callback-literal",
    description: "First argument to error-first callbacks should be an Error object or `null`, not a string literal.",
    remediation: "Pass `new Error('...')` or `null` as the first argument instead of a string literal.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-at",
    description: "Prefer `.at()` method for index access and `String#charAt()`.",
    remediation: "Use `.at(-1)` instead of `[arr.length - 1]` for last-element access, and `str.at(0)` instead of `str.charAt(0)`. The `.at()` method handles negative indices natively.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-pg-enum",
    description: "PostgreSQL `CREATE TYPE ... AS ENUM` — can't remove values once added.",
    remediation: "Replace PG enums with a CHECK constraint (`status TEXT CHECK(status IN ('a','b','c'))`) or a lookup table. PG enums can't have values removed — they're append-only, which makes rollbacks impossible.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-page-click-deprecated",
    description: "`page.click(selector)` is deprecated — use `page.locator(selector).click()`.",
    remediation: "Replace `page.click(selector)` with \
                  `page.locator(selector).click()`. The locator API \
                  auto-waits and auto-retries, and the direct page methods \
                  are deprecated in Playwright.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
--
pub const META: RuleMeta = RuleMeta {
    id: "catch-error-name",
    description: "The catch parameter should be named `error`.",
    remediation: "Rename the catch parameter to `error` (or `error_` if shadowed). \
                  Using `_` is allowed when the parameter is unused.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-boolean",
    description: "Redundant boolean literal in a return or condition.",
    remediation: "Simplify: `if (x) return true; else return false;` \u{2192} `return x;`. `x === true` \u{2192} `x`. The boolean adds no information.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-at",
    description: "Prefer `.at()` method for index access and `String#charAt()`.",
    remediation: "Use `.at(-1)` instead of `[arr.length - 1]` for last-element access, and `str.at(0)` instead of `str.charAt(0)`. The `.at()` method handles negative indices natively.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-dynamic-template",
    description: "Dynamic HTML construction via innerHTML, document.write, or similar APIs is an XSS vector.",
    remediation: "Use safe DOM APIs (`textContent`, `createElement`) or a framework's built-in escaping. Avoid raw HTML injection entirely.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-anonymous-default-export",
    description: "Disallow anonymous functions and classes as the default export.",
    remediation: "Name the exported function or class. Anonymous default \
                  exports break refactoring tools, produce unhelpful stack \
                  traces, and make `import` auto-complete less useful.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-flat",
    description: "Prefer `.flat()` over legacy array flattening techniques.",
    remediation: "Replace `[].concat(…arr)` or `.reduce((a,b) => a.concat(b), [])` with `.flat()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "inverted-assertion-arguments",
    description: "Expected and actual arguments in assertion are inverted.",
    remediation: "Use `expect(variable).toBe(literal)` — the expected value goes in the matcher, not in `expect()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-extra-non-null-assertion",
    description: "Extra non-null assertions (`!!`) are redundant and confusing.",
    remediation: "Remove the extra `!` — a single non-null assertion is sufficient.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-generic-names",
    description: "Generic names carry no meaning.",
    remediation: "Rename to describe what the value IS: `data` → \
                  `parsedOrder`, `info` → `userProfile`, `result` → \
                  `paymentReceipt`, `temp` → name the actual intermediate.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-duplicated-branches",
    description: "Two branches of an if/else have identical bodies.",
    remediation: "Merge the conditions or remove the duplicate branch.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-unified-signatures",
    description: "Function overload signatures that differ by a single parameter should be unified with a union or optional parameter.",
    remediation: "Merge the overload signatures into one using a union type or an optional parameter.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/unified-signatures"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-standalone-expect",
    description: "`expect()` outside a test body never runs as an assertion.",
    remediation: "Move the `expect()` call inside a `test()` or `it()` block.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-standalone-expect.md"),
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "catch-error-name",
    description: "The catch parameter should be named `error`.",
    remediation: "Rename the catch parameter to `error` (or `error_` if shadowed). \
                  Using `_` is allowed when the parameter is unused.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-loop-counter-reassign",
    description: "Assignment to a `for` loop counter inside the loop body causes subtle bugs.",
    remediation: "Use a separate variable instead of reassigning the loop counter. Modifying the counter inside the body makes the loop hard to reason about and often hides off-by-one errors.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-button-has-type",
    description: "`<button>` without an explicit `type` attribute defaults to `submit`, which may cause unexpected form submissions.",
    remediation: "Add an explicit `type` attribute (`button`, `submit`, or `reset`) \
                  to every `<button>` element so the intent is clear.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/button-has-type.md"),
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "expiring-todo-comments",
    description: "TODO/FIXME with an expiration date that has passed should be resolved.",
    remediation: "Resolve the TODO/FIXME — the expiration date has passed. \
                  Either complete the task or update the date.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "data-clumps",
    description: "Same 3+ parameter names appear together in multiple function signatures.",
    remediation: "Extract the repeated parameter group into a value object / options type. Data clumps indicate a missing abstraction — e.g. `(host, port, protocol)` should be a `ConnectionConfig`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "de-morgan-simplify",
    description: "Apply De Morgan's law: `!(a && b)` is `!a || !b`, `!(a || b)` is `!a && !b`.",
    remediation: "Distribute the negation using De Morgan's law. `!(a && b)` becomes `!a || !b` and `!(a || b)` becomes `!a && !b`. The expanded form is easier to reason about.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-dynamic-template",
    description: "Dynamic HTML construction via innerHTML, document.write, or similar APIs is an XSS vector.",
    remediation: "Use safe DOM APIs (`textContent`, `createElement`) or a framework's built-in escaping. Avoid raw HTML injection entirely.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "require-array-join-separator",
    description: "Enforce using the separator argument with `Array#join()`.",
    remediation: "Pass an explicit separator: `arr.join(',')`. The default is `','` but relying on it harms readability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-constructed-context-values",
    description: "`<Provider value={{ ... }}>` creates a new object every render, causing all consumers to re-render.",
    remediation: "Memoize the context value with `useMemo` or extract it to a \
                  stable reference. `<Provider value={memoized}>` avoids \
                  unnecessary re-renders of every consumer.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-anchor-precedence",
    description: "Anchor `^` or `$` in alternation may not bind as expected.",
    remediation: "Wrap the alternation in a group: `/^(a|b)$/` instead of `/^a|b$/`. Without grouping, `/^a|b$/` means `(^a)|(b$)`, not `^(a|b)$`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-in-misuse",
    description:
        "`in` operator on arrays checks keys (indices), not values — use `.includes()` instead.",
    remediation: "Replace `x in arr` with `arr.includes(x)` or use a `Set`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-empty-object-type",
    description: "`{}` as a type matches any non-nullish value — it almost never means what you think.",
    remediation: "Use `Record<string, never>` for an empty object, `object` for any object, \
                  or `unknown` for any value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "banned-comment-words",
    description: "Dismissive filler words in comments hide complexity instead of explaining it.",
    remediation: "Remove the filler word and rewrite the comment to explain the actual \
                  subtlety. If the line is genuinely obvious, delete the comment instead. \
                  Banned: obviously, simply, just, basically, clearly, trivially.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["comments"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-duplicated-branches",
    description: "Two branches of an if/else have identical bodies.",
    remediation: "Merge the conditions or remove the duplicate branch.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-extra-non-null-assertion",
    description: "Extra non-null assertions (`!!`) are redundant and confusing.",
    remediation: "Remove the extra `!` — a single non-null assertion is sufficient.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-needs-description",
    description: "JSDoc block has tags but no description.",
    remediation: "Add a prose description to the JSDoc block. Tags alone don't explain what the function does or why.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-misplaced-loop-counter",
    description: "`for` loop update clause modifies a different variable than the condition.",
    remediation: "Ensure the update expression (`i++`) modifies the same variable used in the loop condition (`i < n`). Mismatched variables usually indicate a copy-paste bug.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-mod-tests-without-cfg-test",
    description: "`mod tests` must be gated by `#[cfg(test)]`.",
    remediation: "Add `#[cfg(test)]` immediately above the `mod tests` \
                  declaration. Without it, every test function ships in \
                  the release binary — bloat plus a risk of pulling in \
                  dev-dependencies that aren't built for release.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-try-promise",
    description: "Promise rejection inside try/catch without `await` won't be caught.",
    remediation: "Add `await` before promise-returning calls inside try blocks, or use `.catch()` directly. Without `await`, the promise rejects asynchronously and the `catch` block never runs.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "expiring-todo-comments",
    description: "TODO/FIXME with an expiration date that has passed should be resolved.",
    remediation: "Resolve the TODO/FIXME — the expiration date has passed. \
                  Either complete the task or update the date.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "option-vs-result",
    description: "Functions named `find*`/`get*` returning `null`/`undefined` should use an Option type.",
    remediation: "Wrap the return value in an Option/Result type instead of returning bare `null` or `undefined`. This makes the absence of a value explicit in the type system.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-cookies-in-layout",
    description: "`cookies()`/`headers()` in a Next.js layout makes ALL child pages dynamic.",
    remediation: "Move `cookies()` / `headers()` calls out of `layout.tsx` into \
                  the individual page files that need them. One call in a layout \
                  forces EVERY child page to be dynamically rendered, defeating \
                  static generation for the entire route segment.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-offset-pagination",
    description: "`OFFSET` pagination is O(N) on deep pages — use cursor-based (keyset) pagination.",
    remediation: "Replace `LIMIT N OFFSET M` with cursor-based pagination: `WHERE id > :last_id ORDER BY id LIMIT N`. OFFSET scans and discards M rows every time.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-complexity",
    description: "Regex pattern is overly complex (score > 20).",
    remediation: "Break the regex into smaller named patterns or use a parser. Complex regex is hard to read, test, and maintain.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-hardcoded-secret-signature",
    description: "Hardcoded secrets in JWT signing or crypto operations leak credentials into source control.",
    remediation: "Load signing keys from environment variables or a secrets manager (e.g., `process.env.JWT_SECRET`).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "import-no-amd",
    description: "AMD `require` and `define` calls are forbidden.",
    remediation: "Use ES module `import` instead of AMD `require([...], fn)` or `define([...], fn)`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/no-amd.md"),
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-keyboard-event-key",
    description: "Prefer `KeyboardEvent#key` over `KeyboardEvent#keyCode`.",
    remediation: "Use `event.key` instead of `event.keyCode`, `event.charCode`, or `event.which`. The `.key` property returns a human-readable string and is the modern standard.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-cookies-in-layout",
    description: "`cookies()`/`headers()` in a Next.js layout makes ALL child pages dynamic.",
    remediation: "Move `cookies()` / `headers()` calls out of `layout.tsx` into \
                  the individual page files that need them. One call in a layout \
                  forces EVERY child page to be dynamically rendered, defeating \
                  static generation for the entire route segment.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
--
pub const META: RuleMeta = RuleMeta {
    id: "inverted-assertion-arguments",
    description: "Expected and actual arguments in assertion are inverted.",
    remediation: "Use `expect(variable).toBe(literal)` — the expected value goes in the matcher, not in `expect()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-set-size",
    description: "Prefer `Set#size` instead of spreading into an array and reading `.length`.",
    remediation: "Replace `[...mySet].length` or `Array.from(mySet).length` \
                  with `mySet.size`. Spreading a Set into an array just to \
                  read its length is wasteful — `Set#size` is O(1).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "require-array-join-separator",
    description: "Enforce using the separator argument with `Array#join()`.",
    remediation: "Pass an explicit separator: `arr.join(',')`. The default is `','` but relying on it harms readability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-identical-functions",
    description: "Two functions have identical implementations.",
    remediation: "Extract the duplicated logic into a shared helper. Identical functions diverge silently when one gets patched.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-between-timestamp",
    description: "`BETWEEN` with timestamps causes off-by-one bugs (inclusive both sides).",
    remediation: "Replace `BETWEEN start AND end` with `>= start AND < end`. BETWEEN is inclusive on both sides — midnight rows get counted twice.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-forward-ref-uses-ref",
    description: "`forwardRef` component does not use the `ref` parameter.",
    remediation: "Either use the `ref` parameter in the component body or remove \
                  the `forwardRef` wrapper — it serves no purpose without a ref.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/forward-ref-uses-ref.md"),
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-ternary",
    description: "Simple if/else assignment can be a ternary expression.",
    remediation: "Replace `if (c) { x = a; } else { x = b; }` with \
                  `x = c ? a : b;`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "zod-no-any",
    description: "`z.any()` disables validation and type narrowing.",
    remediation: "Replace `z.any()` with `z.unknown()`. The runtime \
                  behavior is the same (everything accepted) but the \
                  TypeScript type is `unknown`, forcing downstream code \
                  to narrow before using the value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "zod"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-single-promise-in-promise-methods",
    description: "Wrapping a single-element array with `Promise.all/any/race()` is unnecessary.",
    remediation: "Use the value directly instead of wrapping it in a Promise method: \
                  `await single` instead of `await Promise.all([single])`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-needs-description",
    description: "JSDoc block has tags but no description.",
    remediation: "Add a prose description to the JSDoc block. Tags alone don't explain what the function does or why.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-ternary",
    description: "Simple if/else assignment can be a ternary expression.",
    remediation: "Replace `if (c) { x = a; } else { x = b; }` with \
                  `x = c ? a : b;`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "index-of-compare-to-positive",
    description: "`.indexOf(…) > 0` misses index 0 — use `>= 0` or `!== -1`.",
    remediation: "Replace `> 0` with `>= 0` (or `!== -1`) to include the first element.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-test-logic",
    description: "Tests with `if`/`for`/`while`/`switch` are testing the test, not the code.",
    remediation: "Remove control-flow logic from test bodies. Use \
                  `test.each()` for data-driven tests, extract shared \
                  setup to `beforeEach`, and write one assertion path per \
                  test. Logic in tests hides which branch actually ran, \
                  making failures hard to diagnose.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "node-prefer-promises-fs",
    description: "Callback-based `fs.*` methods are discouraged.",
    remediation: "Use `fs.promises.*` or import from `fs/promises` instead of callback-based `fs` methods.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "option-vs-result",
    description: "Functions named `find*`/`get*` returning `null`/`undefined` should use an Option type.",
    remediation: "Wrap the return value in an Option/Result type instead of returning bare `null` or `undefined`. This makes the absence of a value explicit in the type system.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-try-promise",
    description: "Promise rejection inside try/catch without `await` won't be caught.",
    remediation: "Add `await` before promise-returning calls inside try blocks, or use `.catch()` directly. Without `await`, the promise rejects asynchronously and the `catch` block never runs.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-offset-pagination",
    description: "`OFFSET` pagination is O(N) on deep pages — use cursor-based (keyset) pagination.",
    remediation: "Replace `LIMIT N OFFSET M` with cursor-based pagination: `WHERE id > :last_id ORDER BY id LIMIT N`. OFFSET scans and discards M rows every time.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-reverse",
    description: "`Array#reverse()` mutates the array in place.",
    remediation: "Use `.toReversed()` instead — it returns a new array without mutating the original.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "node-no-top-level-await",
    description: "Top-level `await` is forbidden in published modules.",
    remediation: "Wrap the `await` expression inside an `async` function.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/no-top-level-await.md"),
    categories: &["node"],
};

--
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-single-promise-in-promise-methods",
    description: "Wrapping a single-element array with `Promise.all/any/race()` is unnecessary.",
    remediation: "Use the value directly instead of wrapping it in a Promise method: \
                  `await single` instead of `await Promise.all([single])`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-loop-counter-reassign",
    description: "Assignment to a `for` loop counter inside the loop body causes subtle bugs.",
    remediation: "Use a separate variable instead of reassigning the loop counter. Modifying the counter inside the body makes the loop hard to reason about and often hides off-by-one errors.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "node-no-exports-assign",
    description: "Direct assignment to `exports` variable is forbidden.",
    remediation: "Use `module.exports = ...` instead of `exports = ...`.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/no-exports-assign.md"),
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "import-prefer-default-export",
    description: "Prefer a default export when a module has a single export.",
    remediation: "Use `export default` instead of a single named export.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/prefer-default-export.md"),
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-wrapper-object-types",
    description: "Use lowercase primitives (`string`, `number`, `boolean`) instead of wrapper object types.",
    remediation: "Replace `String` with `string`, `Number` with `number`, `Boolean` with `boolean`, \
                  `Object` with `object`, `Symbol` with `symbol`, `BigInt` with `bigint`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-web-first-assertions",
    description: "`expect(await locator.isVisible()).toBe(true)` does not auto-retry — use web-first assertions.",
    remediation: "Replace `expect(await el.isVisible()).toBe(true)` with \
                  `await expect(el).toBeVisible()`. Web-first assertions \
                  auto-retry until the condition is met or the timeout \
                  expires, making tests more reliable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-script-url",
    description: "`href=\"javascript:...\"` is an XSS vector.",
    remediation: "Use an `onClick` handler instead of a `javascript:` URL. \
                  Script URLs bypass CSP and enable cross-site scripting.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "require-module-attributes",
    description: "Import/export with empty attribute list `with {}` is not allowed.",
    remediation: "Either add the required attributes (e.g. `with { type: 'json' }`) \
                  or remove the empty `with {}` clause.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-max-params",
    description: "Functions with too many parameters are hard to understand and maintain.",
    remediation: "Reduce the number of parameters by using an options object or refactoring.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/max-params"),
    categories: &["typescript"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-math-trunc",
    description: "Prefer `Math.trunc(x)` over bitwise hacks like `x | 0`, `~~x`, or `x >> 0`.",
    remediation: "Replace bitwise truncation with `Math.trunc(x)`. Bitwise operators silently \
                  coerce to 32-bit integers and obscure intent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "todo-needs-issue-link",
    description: "TODO/FIXME/XXX/HACK/BUG without a tracked reference rots into silent tech debt.",
    remediation: "Add an issue reference after the marker — `#123`, `GH-123`, \
                  a ticket key (`ABC-123`), or a full URL. Covers TODO, FIXME, \
                  XXX, HACK, and BUG.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-await-expression-member",
    description: "Do not access a member directly from an await expression.",
    remediation: "Extract the awaited value into a variable, then access the member: \
                  `const response = await fetch(url); const data = response.json();`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-query-selector",
    description: "Prefer `.querySelector()` / `.querySelectorAll()` over legacy DOM query methods.",
    remediation: "Replace `.getElementById('x')` with `.querySelector('#x')`, and `.getElementsByClassName('x')` / `.getElementsByTagName('x')` / `.getElementsByName('x')` with `.querySelectorAll('.x')`. The `querySelector` API is more flexible and consistent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-globals-shadowing",
    description: "Local variable shadows a well-known global identifier.",
    remediation: "Rename the local variable to avoid shadowing `console`, `window`, `document`, `process`, `global`, `globalThis`, `setTimeout`, or `setInterval`. Shadowing globals makes code confusing and can break runtime behavior.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "public-static-readonly",
    description: "`public static` fields without `readonly` allow accidental mutation.",
    remediation: "Add `readonly` to `public static` fields: `public static readonly X = ...`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-raw-locators",
    description: "`page.locator('css-selector')` is brittle — prefer `getByRole`, `getByText`, etc.",
    remediation: "Replace `page.locator('.btn')` with \
                  `page.getByRole('button')` or `page.getByText('Submit')`. \
                  Semantic locators are resilient to markup changes and \
                  align with how users find elements on the page.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "require-module-attributes",
    description: "Import/export with empty attribute list `with {}` is not allowed.",
    remediation: "Either add the required attributes (e.g. `with { type: 'json' }`) \
                  or remove the empty `with {}` clause.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-node-protocol",
    description: "Prefer `node:` protocol for Node.js builtin imports.",
    remediation: "Replace bare builtin specifiers (`fs`, `path`, …) with \
                  `node:fs`, `node:path`. The `node:` prefix makes it \
                  unambiguous that the import targets a Node.js builtin, \
                  not a user-land package with the same name.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-negation-in-equality-check",
    description: "Negated expression in equality check is a precedence bug.",
    remediation: "`!x === y` is parsed as `(!x) === y`, not `!(x === y)`. \
                  Use `x !== y` or wrap explicitly: `!(x === y)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "pure-by-default",
    description: "Function references top-level mutable state.",
    remediation: "Pass the state as a parameter instead of referencing a top-level `let`/`var`. This makes the function pure and easier to test.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-web-first-assertions",
    description: "`expect(await locator.isVisible()).toBe(true)` does not auto-retry — use web-first assertions.",
    remediation: "Replace `expect(await el.isVisible()).toBe(true)` with \
                  `await expect(el).toBeVisible()`. Web-first assertions \
                  auto-retry until the condition is met or the timeout \
                  expires, making tests more reliable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "consistent-destructuring",
    description: "Use destructured variables over properties.",
    remediation: "A property was already destructured from this object — destructure \
                  this property too instead of accessing it via dot notation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-zero-fractions",
    description: "Disallow number literals with zero fractions or dangling dots.",
    remediation: "Remove the unnecessary `.0` fraction — write `1` instead of `1.0`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-built-in-override",
    description: "Overriding built-in globals like `Array`, `Object`, `Promise` shadows critical APIs.",
    remediation: "Rename the variable. Overriding built-in globals breaks standard library behaviour and causes subtle bugs downstream.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "node-prefer-promises-fs",
    description: "Callback-based `fs.*` methods are discouraged.",
    remediation: "Use `fs.promises.*` or import from `fs/promises` instead of callback-based `fs` methods.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "todo-needs-issue-link",
    description: "TODO/FIXME/XXX/HACK/BUG without a tracked reference rots into silent tech debt.",
    remediation: "Add an issue reference after the marker — `#123`, `GH-123`, \
                  a ticket key (`ABC-123`), or a full URL. Covers TODO, FIXME, \
                  XXX, HACK, and BUG.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-quantifier",
    description: "Quantifier can only match once or matches an element that is empty, making it useless.",
    remediation: "Remove the useless quantifier or restructure the pattern.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-useless-quantifier.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-case-label-in-switch",
    description: "Label statement inside switch looks like a case but is a JS label.",
    remediation: "Use `case <value>:` instead. A bare `identifier:` inside a switch is a label statement, not a case branch.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-type-guard",
    description: "Functions named `isX` returning `boolean` with `typeof`/`instanceof` should use type predicates.",
    remediation: "Change the return type from `: boolean` to `: x is Type` to enable type narrowing at call sites.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-raw-locators",
    description: "`page.locator('css-selector')` is brittle — prefer `getByRole`, `getByText`, etc.",
    remediation: "Replace `page.locator('.btn')` with \
                  `page.getByRole('button')` or `page.getByText('Submit')`. \
                  Semantic locators are resilient to markup changes and \
                  align with how users find elements on the page.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
--

--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-misleading-capturing-group",
    description: "Capturing group matches different things at the start and end, which is misleading.",
    remediation: "Restructure the regex so the capturing group has a clear, unambiguous match.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-misleading-capturing-group.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "pure-by-default",
    description: "Function references top-level mutable state.",
    remediation: "Pass the state as a parameter instead of referencing a top-level `let`/`var`. This makes the function pure and easier to test.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "cyclomatic-complexity",
    description: "Functions with cyclomatic complexity > 10 are hard to test and maintain.",
    remediation: "Refactor the function: extract helper functions, use early returns, replace conditionals with polymorphism or lookup tables.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-async-server-action",
    description: "Server actions (functions with `\"use server\"`) must be `async`.",
    remediation: "Add `async` to the function. React Server Actions must be async \
                  functions — a synchronous function with `\"use server\"` will \
                  cause a build error or runtime failure.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-float-for-money",
    description: "Money fields must not be `f32`/`f64` — IEEE 754 rounding errors corrupt totals.",
    remediation: "Use `rust_decimal::Decimal` for arbitrary-precision \
                  monetary values, or a newtype around `i64` representing \
                  the smallest unit (cents, satoshis, …). Floats accumulate \
                  rounding errors and silently break accounting invariants.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-globals-shadowing",
    description: "Local variable shadows a well-known global identifier.",
    remediation: "Rename the local variable to avoid shadowing `console`, `window`, `document`, `process`, `global`, `globalThis`, `setTimeout`, or `setInterval`. Shadowing globals makes code confusing and can break runtime behavior.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-import-module-exports",
    description: "File mixes `import` declarations with `module.exports`.",
    remediation: "Use either ES module syntax (`import`/`export`) or CommonJS (`require`/`module.exports`), not both.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-zero-fractions",
    description: "Disallow number literals with zero fractions or dangling dots.",
    remediation: "Remove the unnecessary `.0` fraction — write `1` instead of `1.0`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-response-static-json",
    description: "Prefer `Response.json()` over `new Response(JSON.stringify())`.",
    remediation: "Replace `new Response(JSON.stringify(data), ...)` with \
                  `Response.json(data, ...)`. The static method sets the \
                  `Content-Type` header automatically and is more readable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-unreadable-array-destructuring",
    description: "Array destructuring may not contain consecutive ignored values.",
    remediation: "Use index access instead: `const third = arr[2]`. \
                  Consecutive commas like `[,, x,,,, y]` are hard to read \
                  and easy to miscount.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-modern-dom-apis",
    description: "Prefer `.before()` / `.replaceWith()` over `.insertBefore()` / `.replaceChild()`.",
    remediation: "Replace `parent.insertBefore(newNode, ref)` with `ref.before(newNode)` \
                  and `parent.replaceChild(newNode, old)` with `old.replaceWith(newNode)`. \
                  The modern APIs are called on the target node directly, removing the \
                  need for a parent reference.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "intermediate-variables",
    description: "Deeply nested expression should be extracted into named intermediate variables.",
    remediation: "Extract sub-expressions into descriptively named local variables to improve readability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "consistent-destructuring",
    description: "Use destructured variables over properties.",
    remediation: "A property was already destructured from this object — destructure \
                  this property too instead of accessing it via dot notation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-case-label-in-switch",
    description: "Label statement inside switch looks like a case but is a JS label.",
    remediation: "Use `case <value>:` instead. A bare `identifier:` inside a switch is a label statement, not a case branch.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-add-event-listener",
    description: "Prefer `.addEventListener()` over `on`-event property assignment.",
    remediation: "Replace `element.onclick = handler` with `element.addEventListener('click', handler)`. `addEventListener` supports multiple listeners and provides better control via options (capture, passive, once).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "node-global-require",
    description: "`require()` calls should be at the top-level module scope.",
    remediation: "Move the `require()` call to the top level of the module.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/global-require.md"),
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-identical-conditions",
    description: "Duplicate condition in `if / else if` chain is always dead code or a bug.",
    remediation: "Change one of the duplicate conditions so each branch is reachable.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-empty-test-fn",
    description: "`#[test] fn x() {}` is a passing stub that exercises nothing.",
    remediation: "Either delete the test or fill it in. An empty test \
                  always passes and gives false confidence that the code \
                  is covered.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "hono-cookie-no-httponly",
    description: "Cookie set without `httpOnly` — accessible to JavaScript (XSS vector).",
    remediation: "Add `httpOnly: true` to cookie options: `setCookie(c, name, value, { httpOnly: true, secure: true, sameSite: 'Lax' })`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-import-module-exports",
    description: "File mixes `import` declarations with `module.exports`.",
    remediation: "Use either ES module syntax (`import`/`export`) or CommonJS (`require`/`module.exports`), not both.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-explicit-module-boundary-types",
    description: "Exported functions and public class methods should have explicit return and parameter types.",
    remediation: "Add explicit return and parameter type annotations to exported functions.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/explicit-module-boundary-types/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "use-type-alias",
    description: "Repeated complex inline type annotations should be extracted into a type alias.",
    remediation: "Create a `type` alias for the repeated annotation and use it in all positions.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-between-timestamp",
    description: "`BETWEEN` with timestamps causes off-by-one bugs (inclusive both sides).",
    remediation: "Replace `BETWEEN start AND end` with `>= start AND < end`. BETWEEN is inclusive on both sides — midnight rows get counted twice.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-promise-resolve-reject",
    description: "Disallow returning `Promise.resolve/reject()` in async functions.",
    remediation: "In an async function, `return value` already wraps in \
                  `Promise.resolve()` and `throw error` already wraps in \
                  `Promise.reject()`. Remove the unnecessary wrapper.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-target-blank",
    description: "`target=\"_blank\"` without `rel=\"noreferrer\"` is a security risk.",
    remediation: "Add `rel=\"noreferrer\"` (or `rel=\"noopener noreferrer\"`) when \
                  using `target=\"_blank\"`. Without it, the opened page can access \
                  `window.opener` and redirect the parent page.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "react-async-server-action",
    description: "Server actions (functions with `\"use server\"`) must be `async`.",
    remediation: "Add `async` to the function. React Server Actions must be async \
                  functions — a synchronous function with `\"use server\"` will \
                  cause a build error or runtime failure.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "regex-no-missing-g-flag",
    description: "Regex used with a method that expects the global flag but the g flag is missing.",
    remediation: "Add the `g` flag to the regex or use a method that does not require it.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-missing-g-flag.html"),
    categories: &["regex"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-slice",
    description: "Prefer `String#slice()` over `String#substr()` and `String#substring()`.",
    remediation: "Replace `.substring()` / `.substr()` with `.slice()`. \
                  `.slice()` has clearer negative-index semantics and is the modern standard.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-trim-start-end",
    description: "Prefer `String#trimStart()` / `String#trimEnd()` over the deprecated `trimLeft()` / `trimRight()`.",
    remediation: "Replace `.trimLeft()` with `.trimStart()` and `.trimRight()` with `.trimEnd()`. \
                  The `trimLeft`/`trimRight` aliases are deprecated in favor of the spec names.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "switch-case-braces",
    description: "Missing braces in `case` clause.",
    remediation: "Wrap `case` clause body in `{ }` to create a block scope. \
                  Without braces, `let`/`const`/`class`/`function` declarations \
                  leak into the enclosing `switch` scope and can cause \
                  `SyntaxError` or surprising variable sharing between cases.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-float-for-money",
    description: "Money fields must not be `f32`/`f64` — IEEE 754 rounding errors corrupt totals.",
    remediation: "Use `rust_decimal::Decimal` for arbitrary-precision \
                  monetary values, or a newtype around `i64` representing \
                  the smallest unit (cents, satoshis, …). Floats accumulate \
                  rounding errors and silently break accounting invariants.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "consistent-assert",
    description: "Prefer `assert.ok(…)` over bare `assert(…)` with `node:assert`.",
    remediation: "Replace bare `assert(…)` calls with `assert.ok(…)` for consistency with the `node:assert` API.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-boolean",
    description: "Redundant boolean literal in a return or condition.",
    remediation: "Simplify: `if (x) return true; else return false;` \u{2192} `return x;`. `x === true` \u{2192} `x`. The boolean adds no information.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-target-blank",
    description: "`target=\"_blank\"` without `rel=\"noreferrer\"` is a security risk.",
    remediation: "Add `rel=\"noreferrer\"` (or `rel=\"noopener noreferrer\"`) when \
                  using `target=\"_blank\"`. Without it, the opened page can access \
                  `window.opener` and redirect the parent page.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "import-no-dynamic-require",
    description: "Calls to `require()` should use string literals.",
    remediation: "Replace the dynamic `require()` argument with a static string literal.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/no-dynamic-require.md"),
    categories: &["imports"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-redeclare",
    description: "Redeclaring a variable in the same scope shadows the previous declaration silently.",
    remediation: "Remove the duplicate declaration or rename the variable.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-redeclare"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-nested-switch",
    description: "`switch` inside another `switch` is hard to follow.",
    remediation: "Extract the inner switch into a separate function. Nested switches create deeply indented, hard-to-read code that is easy to get wrong.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "cognitive-complexity",
    description: "Function cognitive complexity exceeds 5.",
    remediation: "Simplify by extracting helpers, removing nesting, or splitting into smaller functions. Cognitive complexity measures how hard a function is to understand.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unnecessary-type-constraint",
    description: "`<T extends any>` and `<T extends unknown>` are unnecessary — all types already extend these.",
    remediation: "Remove the `extends any` or `extends unknown` constraint.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "switch-case-braces",
    description: "Missing braces in `case` clause.",
    remediation: "Wrap `case` clause body in `{ }` to create a block scope. \
                  Without braces, `let`/`const`/`class`/`function` declarations \
                  leak into the enclosing `switch` scope and can cause \
                  `SyntaxError` or surprising variable sharing between cases.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "ts-ban-tslint-comment",
    description: "TSLint comments are obsolete — the project has been deprecated in favour of ESLint.",
    remediation: "Remove the `tslint:` comment directive.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/ban-tslint-comment/"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-modern-dom-apis",
    description: "Prefer `.before()` / `.replaceWith()` over `.insertBefore()` / `.replaceChild()`.",
    remediation: "Replace `parent.insertBefore(newNode, ref)` with `ref.before(newNode)` \
                  and `parent.replaceChild(newNode, old)` with `old.replaceWith(newNode)`. \
                  The modern APIs are called on the target node directly, removing the \
                  need for a parent reference.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-dbg-macro",
    description: "`dbg!()` is a debugging aid that must not ship.",
    remediation: "Remove the `dbg!()` call. If you need permanent \
                  observability, use `tracing::debug!`/`tracing::info!` \
                  with structured fields instead. `dbg!()` writes to \
                  stderr unconditionally and can leak PII.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-generic-names",
    description: "Generic names carry no meaning.",
    remediation: "Rename to describe what the value IS: `data` → \
                  `parsedOrder`, `info` → `userProfile`, `result` → \
                  `paymentReceipt`, `temp` → name the actual intermediate.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-reject-function-type",
    description: "JSDoc uses bare `Function` or `function` type instead of a specific function signature.",
    remediation: "Replace the bare `Function` type with a specific signature like `{(param: type) => returnType}`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/no-undefined-types.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-query-selector",
    description: "Prefer `.querySelector()` / `.querySelectorAll()` over legacy DOM query methods.",
    remediation: "Replace `.getElementById('x')` with `.querySelector('#x')`, and `.getElementsByClassName('x')` / `.getElementsByTagName('x')` / `.getElementsByName('x')` with `.querySelectorAll('.x')`. The `querySelector` API is more flexible and consistent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-no-unstable-nested-components",
    description: "Component defined inside another component causes unmount/remount every render.",
    remediation: "Move the inner component outside the parent component. Defining a \
                  component inside render means React sees a brand-new type on every \
                  render, destroying the entire subtree's DOM nodes and state.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-wait-for-timeout",
    description: "`waitForTimeout` is a flaky sleep — wait for network or UI state instead.",
    remediation: "Replace `await page.waitForTimeout(ms)` with a web-first \
                  assertion like `await expect(locator).toBeVisible()` or \
                  `await page.waitForResponse(url)`. Fixed sleeps cause \
                  flaky tests on slow CI and waste time on fast machines.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-empty-test-fn",
    description: "`#[test] fn x() {}` is a passing stub that exercises nothing.",
    remediation: "Either delete the test or fill it in. An empty test \
                  always passes and gives false confidence that the code \
                  is covered.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "hono-cookie-no-httponly",
    description: "Cookie set without `httpOnly` — accessible to JavaScript (XSS vector).",
    remediation: "Add `httpOnly: true` to cookie options: `setCookie(c, name, value, { httpOnly: true, secure: true, sameSite: 'Lax' })`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-response-static-json",
    description: "Prefer `Response.json()` over `new Response(JSON.stringify())`.",
    remediation: "Replace `new Response(JSON.stringify(data), ...)` with \
                  `Response.json(data, ...)`. The static method sets the \
                  `Content-Type` header automatically and is more readable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-in-misuse",
    description:
        "`in` operator on arrays checks keys (indices), not values — use `.includes()` instead.",
    remediation: "Replace `x in arr` with `arr.includes(x)` or use a `Set`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-slice",
    description: "Prefer `String#slice()` over `String#substr()` and `String#substring()`.",
    remediation: "Replace `.substring()` / `.substr()` with `.slice()`. \
                  `.slice()` has clearer negative-index semantics and is the modern standard.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "index-of-compare-to-positive",
    description: "`.indexOf(…) > 0` misses index 0 — use `>= 0` or `!== -1`.",
    remediation: "Replace `> 0` with `>= 0` (or `!== -1`) to include the first element.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unused-expressions",
    description: "Expression statements that produce a value but discard it are likely mistakes.",
    remediation: "Assign the result to a variable, use it as a condition, or remove the statement.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-unused-expressions"),
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-useless-promise-resolve-reject",
    description: "Disallow returning `Promise.resolve/reject()` in async functions.",
    remediation: "In an async function, `return value` already wraps in \
                  `Promise.resolve()` and `throw error` already wraps in \
                  `Promise.reject()`. Remove the unnecessary wrapper.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};
--
pub const META: RuleMeta = RuleMeta {
    id: "no-array-reverse",
    description: "`Array#reverse()` mutates the array in place.",
    remediation: "Use `.toReversed()` instead — it returns a new array without mutating the original.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-wait-for-timeout",
    description: "`waitForTimeout` is a flaky sleep — wait for network or UI state instead.",
    remediation: "Replace `await page.waitForTimeout(ms)` with a web-first \
                  assertion like `await expect(locator).toBeVisible()` or \
                  `await page.waitForResponse(url)`. Fixed sleeps cause \
                  flaky tests on slow CI and waste time on fast machines.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-hardcoded-secret-signature",
    description: "Hardcoded secrets in JWT signing or crypto operations leak credentials into source control.",
    remediation: "Load signing keys from environment variables or a secrets manager (e.g., `process.env.JWT_SECRET`).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "cognitive-complexity",
    description: "Function cognitive complexity exceeds 5.",
    remediation: "Simplify by extracting helpers, removing nesting, or splitting into smaller functions. Cognitive complexity measures how hard a function is to understand.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "consistent-assert",
    description: "Prefer `assert.ok(…)` over bare `assert(…)` with `node:assert`.",
    remediation: "Replace bare `assert(…)` calls with `assert.ok(…)` for consistency with the `node:assert` API.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-wrapper-object-types",
    description: "Use lowercase primitives (`string`, `number`, `boolean`) instead of wrapper object types.",
    remediation: "Replace `String` with `string`, `Number` with `number`, `Boolean` with `boolean`, \
                  `Object` with `object`, `Symbol` with `symbol`, `BigInt` with `bigint`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "node-no-new-require",
    description: "`new require('...')` is almost always a bug.",
    remediation: "Separate the `require` call from the `new` expression: `const Mod = require('...'); new Mod()`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "node-no-new-require",
    description: "`new require('...')` is almost always a bug.",
    remediation: "Separate the `require` call from the `new` expression: `const Mod = require('...'); new Mod()`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["node"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-math-trunc",
    description: "Prefer `Math.trunc(x)` over bitwise hacks like `x | 0`, `~~x`, or `x >> 0`.",
    remediation: "Replace bitwise truncation with `Math.trunc(x)`. Bitwise operators silently \
                  coerce to 32-bit integers and obscure intent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unnecessary-type-constraint",
    description: "`<T extends any>` and `<T extends unknown>` are unnecessary — all types already extend these.",
    remediation: "Remove the `extends any` or `extends unknown` constraint.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-clear-text-protocol",
    description: "Clear-text protocol detected — use the encrypted equivalent.",
    remediation: "Replace http:// with https://, ftp:// with sftp://, telnet:// with ssh://. Clear-text protocols transmit data in the open.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "public-static-readonly",
    description: "`public static` fields without `readonly` allow accidental mutation.",
    remediation: "Add `readonly` to `public static` fields: `public static readonly X = ...`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-unbounded-channel",
    description: "Unbounded channels can OOM the process.",
    remediation: "Use `mpsc::channel(N)` (tokio) or `crossbeam::channel::bounded(N)`. \
                  Pick a capacity that bounds memory under load — even \
                  N=1024 is infinitely safer than no bound. The producer \
                  will `.await` (or block) when full, providing backpressure.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "no-fire-event",
    description: "`fireEvent` dispatches a single synthetic event — `userEvent` reproduces the full browser event sequence.",
    remediation: "Replace `fireEvent.click()` / `fireEvent.change()` with \
                  `userEvent.click()` / `userEvent.type()` from \
                  `@testing-library/user-event`. `fireEvent` skips the \
                  intermediate events (keydown, keypress, input) that real \
                  browsers fire, so tests pass but miss event-handler bugs.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-node-protocol",
    description: "Prefer `node:` protocol for Node.js builtin imports.",
    remediation: "Replace bare builtin specifiers (`fs`, `path`, …) with \
                  `node:fs`, `node:path`. The `node:` prefix makes it \
                  unambiguous that the import targets a Node.js builtin, \
                  not a user-land package with the same name.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-native-coercion-functions",
    description: "Prefer using `String`, `Number`, `BigInt`, `Boolean`, and `Symbol` directly.",
    remediation: "Pass the coercion function directly instead of wrapping it: \
                  `.map(Number)` instead of `.map(x => Number(x))`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-identical-conditions",
    description: "Duplicate condition in `if / else if` chain is always dead code or a bug.",
    remediation: "Change one of the duplicate conditions so each branch is reachable.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-built-in-override",
    description: "Overriding built-in globals like `Array`, `Object`, `Promise` shadows critical APIs.",
    remediation: "Rename the variable. Overriding built-in globals breaks standard library behaviour and causes subtle bugs downstream.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-type-guard",
    description: "Functions named `isX` returning `boolean` with `typeof`/`instanceof` should use type predicates.",
    remediation: "Change the return type from `: boolean` to `: x is Type` to enable type narrowing at call sites.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-add-event-listener",
    description: "Prefer `.addEventListener()` over `on`-event property assignment.",
    remediation: "Replace `element.onclick = handler` with `element.addEventListener('click', handler)`. `addEventListener` supports multiple listeners and provides better control via options (capture, passive, once).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-clear-text-protocol",
    description: "Clear-text protocol detected — use the encrypted equivalent.",
    remediation: "Replace http:// with https://, ftp:// with sftp://, telnet:// with ssh://. Clear-text protocols transmit data in the open.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-fire-event",
    description: "`fireEvent` dispatches a single synthetic event — `userEvent` reproduces the full browser event sequence.",
    remediation: "Replace `fireEvent.click()` / `fireEvent.change()` with \
                  `userEvent.click()` / `userEvent.type()` from \
                  `@testing-library/user-event`. `fireEvent` skips the \
                  intermediate events (keydown, keypress, input) that real \
                  browsers fire, so tests pass but miss event-handler bugs.",
    severity: Severity::Warning,
    doc_url: None,
--
pub const META: RuleMeta = RuleMeta {
    id: "rust-unbounded-channel",
    description: "Unbounded channels can OOM the process.",
    remediation: "Use `mpsc::channel(N)` (tokio) or `crossbeam::channel::bounded(N)`. \
                  Pick a capacity that bounds memory under load — even \
                  N=1024 is infinitely safer than no bound. The producer \
                  will `.await` (or block) when full, providing backpressure.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "timeout-on-io",
    description: "I/O calls without a timeout can hang forever.",
    remediation: "Wrap the I/O call with `withTimeout(call, 5_000)` or pass \
                  `{ signal: AbortSignal.timeout(5_000) }`. Default \
                  timeouts: 5s for DB, 10s for external APIs, 30s for file ops.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
--
pub const META: RuleMeta = RuleMeta {
    id: "require-post-message-target-origin",
    description: "`postMessage()` called without the `targetOrigin` argument.",
    remediation: "Always provide a `targetOrigin` argument (e.g. `self.location.origin` or `'*'`) to `postMessage()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "prefer-native-coercion-functions",
    description: "Prefer using `String`, `Number`, `BigInt`, `Boolean`, and `Symbol` directly.",
    remediation: "Pass the coercion function directly instead of wrapping it: \
                  `.map(Number)` instead of `.map(x => Number(x))`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "timeout-on-io",
    description: "I/O calls without a timeout can hang forever.",
    remediation: "Wrap the I/O call with `withTimeout(call, 5_000)` or pass \
                  `{ signal: AbortSignal.timeout(5_000) }`. Default \
                  timeouts: 5s for DB, 10s for external APIs, 30s for file ops.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
--
pub const META: RuleMeta = RuleMeta {
    id: "require-number-to-fixed-digits-argument",
    description: "Enforce using the digits argument with `Number#toFixed()`.",
    remediation: "Pass an explicit digits argument: `num.toFixed(0)`. The default is `0` but relying on it harms readability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "rust-no-dbg-macro",
    description: "`dbg!()` is a debugging aid that must not ship.",
    remediation: "Remove the `dbg!()` call. If you need permanent \
                  observability, use `tracing::debug!`/`tracing::info!` \
                  with structured fields instead. `dbg!()` writes to \
                  stderr unconditionally and can leak PII.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
--
pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-valid-types",
    description: "JSDoc type expressions must be syntactically valid.",
    remediation: "Fix the malformed type expression — ensure braces match and type names are valid.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/valid-types.md"),
    categories: &["jsdoc"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "react-self-closing-comp",
    description: "Components and HTML elements without children should use self-closing syntax.",
    remediation: "Replace `<Foo></Foo>` with `<Foo />` (and `<div></div>` with `<div />` \
                  in JSX). This reduces noise and makes it obvious the element has no content.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/self-closing-comp.md"),
    categories: &["react"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "no-nested-switch",
    description: "`switch` inside another `switch` is hard to follow.",
    remediation: "Extract the inner switch into a separate function. Nested switches create deeply indented, hard-to-read code that is easy to get wrong.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "intermediate-variables",
    description: "Deeply nested expression should be extracted into named intermediate variables.",
    remediation: "Extract sub-expressions into descriptively named local variables to improve readability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

--
pub const META: RuleMeta = RuleMeta {
    id: "require-number-to-fixed-digits-argument",
    description: "Enforce using the digits argument with `Number#toFixed()`.",
    remediation: "Pass an explicit digits argument: `num.toFixed(0)`. The default is `0` but relying on it harms readability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

--
