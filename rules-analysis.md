# Analyse des règles ESLint pour comply

Analyse complète de 84 plugins ESLint. Chaque règle est évaluée avec une recommandation et une justification.

---

## Légende

- ✅ **IMPLÉMENTER** — Règle à haute valeur ajoutée
- ⚠️ **CONSIDÉRER** — Utile mais opt-in ou effort élevé
- ❌ **NE PAS IMPLÉMENTER** — Doublon, nécessite types TS, ou trop spécifique

---

# PARTIE 1 : RÈGLES À IMPLÉMENTER

## 1.1 Sécurité

### ✅ `no-postmessage-star-origin`

**Source:** eslint-plugin-sdl  
**Pourquoi:** `window.postMessage(data, '*')` envoie des données à n'importe quelle origine. C'est une fuite de données cross-origin triviale à exploiter. Pattern très simple à détecter : chercher `postMessage` avec `'*'` comme second argument.

### ✅ `no-document-domain`
**Source:** eslint-plugin-sdl  
**Pourquoi:** Assigner `document.domain` affaiblit la politique same-origin du navigateur. C'est une technique obsolète qui ouvre des vecteurs d'attaque XSS. Détection triviale par AST.

### ✅ `no-document-write`
**Source:** eslint-plugin-sdl  
**Pourquoi:** `document.write()` est un vecteur XSS classique et bloque le parser HTML. Google Lighthouse le signale comme problème de performance. Simple à détecter.

### ✅ `no-inner-html`
**Source:** eslint-plugin-sdl  
**Pourquoi:** `element.innerHTML = userInput` est la source #1 de XSS dans le code DOM. Comply a déjà `no-dangerously-set-inner-html` pour React, mais pas pour le DOM vanilla. Cette règle comble le gap.

### ✅ `no-insecure-url`
**Source:** eslint-plugin-sdl  
**Pourquoi:** Les URLs `http://` dans le code source sont des vecteurs MITM. Facile à détecter via regex sur les string literals. Autofix possible vers `https://`.

### ✅ `no-unsafe-alloc`
**Source:** eslint-plugin-sdl  
**Pourquoi:** `Buffer.allocUnsafe()` et `new Buffer(size)` allouent de la mémoire non-initialisée qui peut contenir des données sensibles d'opérations précédentes. Fuite de secrets potentielle.

### ✅ `detect-child-process`
**Source:** eslint-plugin-security-node  
**Pourquoi:** `child_process.exec(userInput)` est une injection de commande shell. Comply a `no-shell-exec` mais cette règle ajoute la détection de flux de données depuis l'input utilisateur.

REVIEW: mieux vaut avoir celle là et supprime no-shell-exec non ?

### ✅ `detect-dangerous-redirects`

**Source:** eslint-plugin-security-node  
**Pourquoi:** `res.redirect(req.query.url)` est un open redirect. Comply a `no-unvalidated-url-redirect` mais cette règle couvre les patterns Express spécifiques.

### ✅ `detect-eval-with-expr`
**Source:** eslint-plugin-security-node  
**Pourquoi:** Plus précis que le blanket `no-eval`. Flag seulement `eval(variable)` mais permet `eval('literal')` dans les cas légitimes (rare mais existant). 

REVIEW: pas besoin si on a déjà le no-eval

### ✅ `detect-non-literal-require`
**Source:** eslint-plugin-security-node  
**Pourquoi:** `require(userInput)` permet l'injection de module/path. Attaque classique en Node.js. Détection simple : `require()` avec argument non-literal.

REVIEW: on a pas déjà une règle qui interdit le commonJS ?

### ✅ `detect-option-rejectunauthorized`

**Source:** eslint-plugin-security-node  
**Pourquoi:** `{ rejectUnauthorized: false }` désactive la vérification TLS. Comply a `no-weak-ssl` mais cette règle couvre les APIs Node `https`/`tls` spécifiquement.

### ✅ `no-unsafe-regex`

**Source:** eslint-plugin-redos-detector  
**Pourquoi:** Détecte les regex vulnérables aux attaques ReDoS (Denial of Service). Comply a `regex-complexity` mais ReDoS est une classe de vulnérabilité distincte qui nécessite une analyse spécifique des backtracking patterns.

### ✅ `react-no-javascript-urls`
**Source:** eslint-plugin-react-security  
**Pourquoi:** `href="javascript:..."` est un vecteur XSS en JSX. Comply a `no-script-url` pour JS vanilla mais pas la version JSX-attribute.

### ✅ `html-no-target-blank`
**Source:** html-eslint  
**Pourquoi:** `<a target="_blank">` sans `rel="noopener noreferrer"` permet le tabnabbing. Comply couvre JSX mais pas les fichiers HTML purs.

REVIEW: c'est toujours d'actualité ? je croyais que les browsers moderns n'en avaient plus besoin ?

---

## 1.2 Async / Promises

### ✅ `no-floating-promise`
**Source:** eslint-plugin-ai-guard, eslint-plugin-no-floating-promise  
**Pourquoi:** Une Promise non-awaited et non-catchée est un bug silencieux. Les rejections sont perdues, les erreurs ne remontent pas. C'est l'une des sources de bugs les plus fréquentes en JS moderne. Détectable par AST sans type info pour les cas évidents (call expression en statement).

REVIEW: ok mais on a pas déjà une regle ?

### ✅ `no-async-without-await`
**Source:** eslint-plugin-ai-guard  
**Pourquoi:** Une fonction `async` sans `await` est suspecte — soit le `await` a été oublié, soit le `async` est inutile. Dans les deux cas, c'est un code smell qui mérite un warning.

### ✅ `no-async-array-callback`
**Source:** eslint-plugin-ai-guard  
**Pourquoi:** `array.forEach(async (item) => { ... })` ne wait pas les callbacks. Les rejections sont silencieusement perdues. Bug extrêmement courant. Même pattern avec `map`/`filter` quand le résultat n'est pas awaité avec `Promise.all`.

### ✅ `no-redundant-await`
**Source:** eslint-plugin-ai-guard  
**Pourquoi:** `return await x` à la fin d'une fonction async est inutile (sauf dans try/catch). Micro-optimisation mais surtout indicateur de compréhension des Promises.

---

## 1.3 Error Handling

### ✅ `throw-error-values`
**Source:** eslint-plugin-etc  
**Pourquoi:** `throw 'message'` ou `throw { error: true }` casse la stack trace et empêche `error instanceof Error`. Bug de débutant très courant. Détectable par AST pour les literals.

### ✅ `exception-use-error-cause`
**Source:** eslint-plugin-exception-handling  
**Pourquoi:** Quand on re-throw une erreur, `new Error(msg, { cause: originalErr })` préserve la chaîne de causalité. Sans ça, on perd la stack trace originale. ES2022+ feature très sous-utilisée.

### ✅ `try-catch-json-parse`
**Source:** eslint-plugin-try-catch-failsafe  
**Pourquoi:** `JSON.parse()` throw sur input invalide. C'est l'une des causes de crash les plus fréquentes en production. Require try/catch ou wrapper safe.

### ✅ `try-catch-new-url`
**Source:** eslint-plugin-try-catch-failsafe  
**Pourquoi:** `new URL(input)` throw sur URL invalide. Même pattern que JSON.parse — crash fréquent en prod sur input utilisateur.

### ✅ `no-catch-log-rethrow`
**Source:** eslint-plugin-ai-guard  
**Pourquoi:** `catch(e) { console.log(e); throw e; }` ne fait que dupliquer les logs. Soit on handle l'erreur, soit on la laisse remonter. Ce pattern crée du bruit dans les logs.

### ✅ `no-catch-without-use`

**Source:** eslint-plugin-ai-guard  
**Pourquoi:** `catch(e) { doSomething(); }` sans utiliser `e` swallow silencieusement l'erreur. Comply a `no-empty-catch` mais ça ne couvre pas les catch avec du code qui ignore l'erreur.

---

## 1.4 Imports / Architecture

### ✅ `import-no-cycle`
**Source:** eslint-plugin-import-x  
**Pourquoi:** Les imports cycliques créent des bugs subtils (modules partiellement initialisés), compliquent le tree-shaking, et indiquent une architecture mal découpée. Comply a `ImportIndex` pour la détection cross-file.

### ✅ `import-no-extraneous-dependencies`
**Source:** eslint-plugin-import-x  
**Pourquoi:** Importer un package qui n'est pas dans `package.json` (dépendance transitive) casse quand la dépendance parente change. Bug classique en monorepo.

### ✅ `import-no-restricted-paths`
**Source:** eslint-plugin-import-x  
**Pourquoi:** Enforce les boundaries architecturales (domain ne peut pas importer infrastructure). Foundation pour hexagonal/clean architecture. Configurable par projet.

REVIEW: supprime

### ✅ `avoid-importing-barrel-files`

**Source:** eslint-plugin-barrel-files  
**Pourquoi:** Comply a `avoid-barrel-files` (côté authoring) mais pas côté consumer. Importer depuis un barrel file tire tout le bundle même si on n'utilise qu'une fonction.

### ✅ `import-dedupe`
**Source:** eslint-plugin-antfu  
**Pourquoi:** `import { a, a } from 'x'` après un mauvais merge/rebase. Erreur runtime. Simple à détecter, autofix possible.

### ✅ `no-full-import`
**Source:** eslint-plugin-small-import  
**Pourquoi:** `import _ from 'lodash'` tire 70kb. `import kebabCase from 'lodash/kebabCase'` tire 2kb. Impact bundle majeur. Pattern simple à détecter.

### ✅ `no-test-imports-in-prod`
**Source:** eslint-plugin-fast-import  
**Pourquoi:** Importer des fichiers `*.test.ts`, `__mocks__/*`, ou fixtures depuis du code prod pollue le bundle et peut exposer des helpers de test. Cross-file analysis via ImportIndex.

### ✅ `require-path-exists`
**Source:** eslint-plugin-require-path-exists  
**Pourquoi:** Valide que les chemins `import`/`require` existent sur disque avant le build. Catch les erreurs de typo et fichiers supprimés. Très bas taux de faux positifs.

---

## 1.5 React

### ✅ `jsx-no-new-function-as-prop`
**Source:** eslint-plugin-react-perf  
**Pourquoi:** `<Button onClick={() => doSomething()} />` crée une nouvelle fonction à chaque render, cassant `React.memo` et causant des re-renders inutiles. Comply a `jsx-no-new-array-as-prop` et `jsx-no-new-object-as-prop` mais pas la version function.

REVIEW: pourtant c'est un pattern qu'on voit partout, tu es sûr que react ne gère pas ces cas proprement ?

### ✅ `jsx-ensure-booleans`
**Source:** eslint-plugin-jsx-conditionals  
**Pourquoi:** `{items.length && <List />}` rend `0` dans le DOM quand le tableau est vide. Bug très courant. Require `{items.length > 0 && ...}` ou `{!!items.length && ...}`.

### ✅ `react-no-adjust-state-on-prop-change`
**Source:** eslint-plugin-react-you-might-not-need-an-effect  
**Pourquoi:** `useEffect(() => { setState(derive(props)) }, [props])` est un antipattern. Il faut dériver pendant le render, pas dans un effect. Cause des double-renders et des bugs de timing.

### ✅ `react-no-pass-data-to-parent`
**Source:** eslint-plugin-react-you-might-not-need-an-effect  
**Pourquoi:** `useEffect(() => { onDataChange(data) }, [data])` pour "remonter" des données au parent est un antipattern. Lift state up proprement ou utiliser un context.

### ✅ `react-no-reset-all-state-on-prop-change`
**Source:** eslint-plugin-react-you-might-not-need-an-effect  
**Pourquoi:** `useEffect(() => { resetAllState() }, [id])` est un antipattern. Utiliser `<Component key={id} />` pour reset automatiquement. Pattern documenté dans les docs React.

### ✅ `react-no-chain-state-updates`
**Source:** eslint-plugin-react-you-might-not-need-an-effect  
**Pourquoi:** `useEffect(() => { setA(x); setB(y); }, [a])` cause des cascades de re-renders. Un seul state ou reducer est préférable.

### ✅ `react-hook-form-destructuring-formstate`
**Source:** eslint-plugin-react-hook-form  
**Pourquoi:** `formState.isValid` sans destructurer ne s'abonne pas correctement aux updates RHF. Bug silencieux où le formulaire ne réagit pas aux changements. Très spécifique mais très impactant pour les utilisateurs RHF.

### ✅ `no-submit-handler-without-preventDefault`
**Source:** eslint-plugin-upleveled  
**Pourquoi:** `onSubmit={(e) => { ... }}` sans `e.preventDefault()` cause un page reload. Bug #1 des débutants React. Facile à détecter : chercher `onSubmit` handler sans `preventDefault` call.

---

## 1.6 Database

### ✅ `enforce-delete-with-where`
**Source:** eslint-plugin-drizzle  
**Pourquoi:** `db.delete(users)` sans `.where()` supprime toute la table. Catastrophique en prod. Comply a les règles Drizzle mais pas celle-ci.

### ✅ `enforce-update-with-where`
**Source:** eslint-plugin-drizzle  
**Pourquoi:** `db.update(users).set({ banned: true })` sans `.where()` update toute la table. Même pattern que delete.

### ✅ `pg-require-limit`
**Source:** eslint-plugin-postgresql  
**Pourquoi:** SELECT sans LIMIT sur une grosse table = OOM ou timeout. Comply a `drizzle-no-select-without-limit` mais pas pour les queries SQL raw/pg.

---

## 1.7 Testing

### ✅ `playwright-missing-playwright-await`
**Source:** eslint-plugin-playwright  
**Pourquoi:** Oublier `await` sur `page.click()` ou `expect()` est un bug de test silencieux. Le test passe mais ne vérifie rien. Très courant.

### ✅ `playwright-expect-expect`
**Source:** eslint-plugin-playwright  
**Pourquoi:** Un test sans assertion est un test qui ne teste rien. Require au moins un `expect()` par `test()`.

### ✅ `playwright-no-eval`
**Source:** eslint-plugin-playwright  
**Pourquoi:** `page.$eval()` et `page.$$eval()` sont legacy. Prefer `page.locator()` qui a meilleur retry et error messages.

### ✅ `playwright-prefer-locator`
**Source:** eslint-plugin-playwright  
**Pourquoi:** `page.$()` et `page.$$()` retournent des ElementHandles qui ne retry pas. `page.locator()` est la bonne abstraction.

### ✅ `vitest-hoisted-apis-on-top`
**Source:** eslint-plugin-vitest  
**Pourquoi:** `vi.mock()` et `vi.hoisted()` doivent être avant les imports pour fonctionner. Bug de hoisting subtil spécifique à Vitest.

### ✅ `vitest-no-disabled-tests`
**Source:** eslint-plugin-vitest  
**Pourquoi:** `test.skip()` et `xtest()` pourrissent. Comply a `no-focused-test` pour les `.only` mais pas pour les skips.

---

## 1.8 TypeScript

### ✅ `ts-consistent-type-imports`
**Source:** typescript-eslint  
**Pourquoi:** `import type { X }` permet au bundler de drop l'import complètement. Sans ça, le bundler doit garder l'import "au cas où" il y aurait des side effects. Impact bundle réel.

### ✅ `ts-consistent-type-exports`
**Source:** typescript-eslint  
**Pourquoi:** Symétrique à consistent-type-imports. `export type { X }` pour les exports type-only.

### ✅ `ts-no-non-null-assertion`
**Source:** typescript-eslint  
**Pourquoi:** `value!` dit "je sais que c'est pas null" mais c'est souvent faux. Cache des bugs potentiels. Prefer optional chaining ou checks explicites.

### ✅ `ts-only-throw-error`
**Source:** typescript-eslint  
**Pourquoi:** Même logique que `throw-error-values` mais pour TypeScript. Détectable par AST pour les cas literals.

### ✅ `ts-prefer-promise-reject-errors`
**Source:** typescript-eslint  
**Pourquoi:** `Promise.reject('message')` casse la stack trace. Toujours reject avec un Error.

---

## 1.9 HTML / Accessibility

### ✅ `html-no-abstract-roles`
**Source:** html-eslint  
**Pourquoi:** Les rôles ARIA abstraits (widget, landmark, structure) ne doivent jamais être utilisés directement. Erreur a11y qui casse les lecteurs d'écran.

### ✅ `html-no-aria-hidden-body`
**Source:** html-eslint  
**Pourquoi:** `aria-hidden="true"` sur `<body>` rend toute la page invisible aux lecteurs d'écran. Catastrophique.

### ✅ `html-no-nested-interactive`
**Source:** html-eslint  
**Pourquoi:** `<button><a href="...">` ou `<a><button>` est du HTML invalide qui casse la navigation clavier.

### ✅ `html-no-skip-heading-levels`
**Source:** html-eslint  
**Pourquoi:** Passer de `<h1>` à `<h3>` sans `<h2>` casse la structure du document pour les lecteurs d'écran.

### ✅ `html-no-positive-tabindex`
**Source:** html-eslint  
**Pourquoi:** `tabindex="5"` casse l'ordre de tabulation naturel. Seuls `0` et `-1` sont valides.

### ✅ `html-no-invalid-attr-value`
**Source:** html-eslint  
**Pourquoi:** `<a target="blank">` (sans underscore) est silencieusement ignoré. Catch les typos dans les valeurs d'attributs énumérés.

### ✅ `html-require-button-type`
**Source:** html-eslint  
**Pourquoi:** `<button>` sans `type` default à `submit` dans un form. Cause des soumissions accidentelles. Toujours spécifier `type="button"` ou `type="submit"`.

### ✅ `html-require-img-alt`
**Source:** html-eslint  
**Pourquoi:** Comply a `a11y-alt-text` pour JSX mais pas pour HTML pur. Les fichiers `.html` ont besoin de la même règle.

### ✅ `html-require-input-label`
**Source:** html-eslint  
**Pourquoi:** Input sans label associé est inaccessible. Version HTML de `a11y-label-has-associated-control`.

### ✅ `html-require-explicit-size`
**Source:** html-eslint  
**Pourquoi:** Images/iframes/videos sans `width`/`height` causent du Cumulative Layout Shift (CLS). Impact direct sur Core Web Vitals.

---

## 1.10 Performance

### ✅ `require-size-attributes`
**Source:** eslint-plugin-layout-shift  
**Pourquoi:** Même règle que `html-require-explicit-size` mais pour JSX. `<img>` et `<video>` sans dimensions causent du CLS.

---

## 1.11 i18n

### ✅ `i18n-json-identical-keys`
**Source:** eslint-plugin-i18n-json  
**Pourquoi:** Vérifier que tous les fichiers de locale (`en.json`, `fr.json`, etc.) ont les mêmes clés. Catch les traductions manquantes au lint-time plutôt qu'en prod.

### ✅ `i18n-json-identical-placeholders`

**Source:** eslint-plugin-i18n-json  
**Pourquoi:** `{{name}}` dans `en.json` mais `{{nom}}` dans `fr.json` cause un crash runtime. Les placeholders doivent matcher.

### ✅ `i18n-json-valid-message-syntax`
**Source:** eslint-plugin-i18n-json  
**Pourquoi:** Syntaxe ICU malformée dans les fichiers de traduction cause des crashes. Valider au lint-time.

---

## 1.12 Code Quality

### ✅ `no-one-iteration-loop`
**Source:** eslint-plugin-radar  
**Pourquoi:** Une boucle qui `return`/`break`/`throw` inconditionnellement ne peut itérer qu'une fois. C'est presque toujours un bug de copier-coller.

### ✅ `no-extra-arguments`
**Source:** eslint-plugin-radar  
**Pourquoi:** Appeler `fn(a, b, c)` quand `fn` ne prend que 2 params. Les args supplémentaires sont silencieusement ignorés. Bug probable.

### ✅ `no-use-of-empty-return-value`
**Source:** eslint-plugin-radar  
**Pourquoi:** `const x = console.log('hi')` assigne `undefined`. Détectable sans type info en trackant les fonctions sans return dans le même fichier.

### ✅ `prefer-early-return`
**Source:** eslint-plugin-prefer-early-return  
**Pourquoi:** Les guard clauses rendent le code plus lisible. `if (!valid) return;` plutôt que `if (valid) { ... tout le code ... }`.

### ✅ `block-scope-case`
**Source:** @blitz/eslint-plugin  
**Pourquoi:** Déclarer une variable dans un `case` sans `{}` la hoist à tout le switch. Bug de scoping classique.



-> REVIEW: est-ce qu'on peut faire une règle qui limite le nombre d'indirection dans une fonction ? Si par exemple une fonction appelle une fonction qui appelle une fonction qui appelle un fonction (indirections) : error

---

# PARTIE 2 : RÈGLES À CONSIDÉRER (OPT-IN)

## 2.1 Immutabilité / FP

### ⚠️ `no-mutation`
**Source:** eslint-plugin-const-immutable  
**Pourquoi utile:** Enforce l'immutabilité sur les `const` bindings. `const arr = []; arr.push(x);` serait flaggé.  
**Pourquoi opt-in:** Trop strict pour la plupart des codebases. Utile pour les équipes qui veulent un style FP strict.

REVIEW: à implémenter

### ⚠️ `no-mutating-methods`

**Source:** eslint-plugin-fp  
**Pourquoi utile:** Flag `.push()`, `.pop()`, `.sort()`, `.reverse()`, `.splice()`. Ces méthodes mutent en place.  
**Pourquoi opt-in:** Beaucoup de code légitime utilise ces méthodes. Réservé aux codebases FP.

REVIEW: à implémenter

### ⚠️ `functional-immutable-data`

**Source:** eslint-plugin-functional  
**Pourquoi utile:** Interdit toute mutation de variables déclarées ailleurs.  
**Pourquoi opt-in:** Très restrictif. Pour les projets qui veulent du full-immutable.

REVIEW: donne moi des exemples où ça serait légitime stp

### ⚠️ `no-delete`

**Source:** eslint-plugin-fp  
**Pourquoi utile:** `delete obj.prop` déoptimise les hidden classes V8. Préférer rest-spread.  
**Pourquoi opt-in:** Impact perf réel mais rare. Code smell plutôt qu'erreur.

REVIEW: à implémenter

---

## 2.2 Tailwind

### ⚠️ `enforce-shorthand-classes`
**Source:** eslint-plugin-better-tailwindcss  
**Pourquoi utile:** `h-4 w-4` → `size-4`. DX et classes plus courtes.  
**Pourquoi opt-in:** Comply a déjà `tailwind-prefer-size-shorthand`. Vérifier overlap avant d'ajouter.

REVIEW: ok vérifie

### ⚠️ `tailwind-no-deprecated-classes`
**Source:** eslint-plugin-better-tailwindcss  
**Pourquoi utile:** Flag les classes Tailwind v2/v3 qui ont changé en v4.  
**Pourquoi opt-in:** Utile pendant migration, moins après.

REVIEW: à implémenter

### ⚠️ `enforce-logical-properties`

**Source:** eslint-plugin-better-tailwindcss  
**Pourquoi utile:** `ms-`/`me-` au lieu de `ml-`/`mr-` pour le support RTL.  
**Pourquoi opt-in:** Seulement pour les apps internationales avec RTL.

REVIEW: à ne pas faire

### ⚠️ `tailwind-classnames-order`
**Source:** eslint-plugin-tailwindcss  
**Pourquoi utile:** Ordre canonique des classes pour consistance.  
**Pourquoi opt-in:** Plus une préférence d'équipe. Prettier-plugin-tailwindcss fait ça aussi.

REVIEW: à implémenter

### ⚠️ `tailwind-no-custom-classname`
**Source:** eslint-plugin-tailwindcss  
**Pourquoi utile:** Catch les typos dans les noms de classe.  
**Pourquoi opt-in:** Besoin de connaître la config Tailwind du projet pour éviter les faux positifs.

REVIEW: ne pas faire

---

## 2.3 Architecture

### ⚠️ `boundaries-element-types`
**Source:** eslint-plugin-boundaries  
**Pourquoi utile:** Enforce quels "types" de modules peuvent importer quoi. Plus flexible que FSD.  
**Pourquoi opt-in:** Nécessite configuration par projet. Pas de default universel.

REVIEW: ne pas faire

### ⚠️ `boundaries-external`

**Source:** eslint-plugin-boundaries  
**Pourquoi utile:** Restrict quels packages externes chaque layer peut utiliser.  
**Pourquoi opt-in:** Même raison — très projet-spécifique.

REVIEW: ne pas faire

### ⚠️ `index-only-import-export`
**Source:** eslint-plugin-index  
**Pourquoi utile:** Force les fichiers `index.ts` à être des barrels purs (pas de logique).  
**Pourquoi opt-in:** Convention d'équipe, pas universel.

REVIEW: ne pas faire on ne veut pas de barrels

---

## 2.4 Node / Package.json

### ⚠️ `pkg-no-dupe-deps`
**Source:** eslint-plugin-node-dependencies  
**Pourquoi utile:** Un package dans dependencies ET devDependencies est une erreur.  
**Pourquoi opt-in:** Facile à implémenter mais impact limité (npm/pnpm gèrent).

REVIEW: à faire

### ⚠️ `pkg-valid-semver`
**Source:** eslint-plugin-node-dependencies  
**Pourquoi utile:** Catch les versions malformées avant npm install.  
**Pourquoi opt-in:** npm le catch déjà à l'install. Valeur limitée.

REVIEW: à ne pas faire

### ⚠️ `pkg-absolute-version`
**Source:** eslint-plugin-node-dependencies  
**Pourquoi utile:** Interdit `^`/`~` pour lock les versions exactes.  
**Pourquoi opt-in:** Philosophie lockfile vs version ranges. Très opinionated.

REVIEW: à ne pas faire

---

## 2.5 Zod

### ⚠️ `zod-no-optional-and-default-together`
**Source:** eslint-plugin-zod  
**Pourquoi utile:** `.optional().default(x)` est redondant — default implique optional.  
**Pourquoi opt-in:** Bug mineur, pas de conséquence runtime.

REVIEW: à faire

### ⚠️ `zod-no-unknown-schema`
**Source:** eslint-plugin-zod  
**Pourquoi utile:** `z.unknown()` est un escape hatch qui bypass la validation.  
**Pourquoi opt-in:** Parfois nécessaire. Opt-in avec whitelist.

REVIEW: à faire

### ⚠️ `zod-require-schema-suffix`
**Source:** eslint-plugin-zod  
**Pourquoi utile:** Nommer les schemas avec suffix `...Schema` pour les identifier facilement.  
**Pourquoi opt-in:** Convention de naming, pas universel.

REVIEW: à faire

---

## 2.6 XState

### ⚠️ `xstate-entry-exit-action`
**Source:** eslint-plugin-xstate  
**Pourquoi utile:** Valide la structure des actions entry/exit.  
**Pourquoi opt-in:** Niche — seulement pour les utilisateurs XState.

REVIEW: à faire

### ⚠️ `xstate-invoke-usage`
**Source:** eslint-plugin-xstate  
**Pourquoi utile:** Valide les configs invoke (src, onDone, onError).  
**Pourquoi opt-in:** Même raison.

REVIEW: à faire

### ⚠️ `xstate-no-imperative-action`
**Source:** eslint-plugin-xstate  
**Pourquoi utile:** Interdit send/raise hors des action creators.  
**Pourquoi opt-in:** Spécifique XState v5.

REVIEW: à faire si c'est la derniere version

---

## 2.7 Vitest (compléments)

### ⚠️ `vitest-no-duplicate-hooks`
**Source:** eslint-plugin-vitest  
**Pourquoi utile:** Deux `beforeEach` dans le même describe — le second override silencieusement.  
**Pourquoi opt-in:** Comply a des règles similaires pour Jest/Playwright.

REVIEW: à faire

### ⚠️ `vitest-no-large-snapshots`
**Source:** eslint-plugin-vitest  
**Pourquoi utile:** Snapshots > N lignes cachent les régressions.  
**Pourquoi opt-in:** Threshold configurable, pas de bon default.

REVIEW: à ne pas faire

### ⚠️ `vitest-require-test-timeout`
**Source:** eslint-plugin-vitest  
**Pourquoi utile:** Tests async longs sans timeout bloquent CI.  
**Pourquoi opt-in:** Patterns de détection complexes.

REVIEW: à ne pas faire

---

## 2.8 Code Quality (mineurs)

### ⚠️ `prefer-single-boolean-return`
**Source:** eslint-plugin-radar  
**Pourquoi utile:** `if (x) return true; return false;` → `return x;`  
**Pourquoi opt-in:** Autofix safe mais stylistic.

REVIEW: à faire

### ⚠️ `proper-arrows-return`
**Source:** eslint-plugin-proper-arrows  
**Pourquoi utile:** Flag les arrow concise-body confus (objects sans parens, ternaires).  
**Pourquoi opt-in:** Overlap avec d'autres règles.

REVIEW: à ne pas faire si overlap

### ⚠️ `visual-complexity`
**Source:** eslint-plugin-visual-complexity  
**Pourquoi utile:** Variante de cognitive complexity focalisée sur la lisibilité visuelle.  
**Pourquoi opt-in:** Comply a déjà cognitive-complexity et cyclomatic-complexity.

REVIEW: ça apporte quoi en plus ?

### ⚠️ `no-broad-exception`
**Source:** eslint-plugin-ai-guard  
**Pourquoi utile:** `catch(e) { }` sans narrowing attrape tout, y compris les bugs.  
**Pourquoi opt-in:** Heuristique imparfaite. Beaucoup de faux positifs potentiels.

REVIEW: pas compris ce que ça faisait ?

---

# PARTIE 3 : RÈGLES À NE PAS IMPLÉMENTER

## 3.1 Doublons avec comply

### ❌ `avoid-barrel-files`
**Pourquoi non:** Déjà dans comply.

### ❌ `avoid-re-export-all`
**Pourquoi non:** Déjà dans comply.

### ❌ `de-morgan-simplify`
**Pourquoi non:** Déjà dans comply.

### ❌ `no-commented-out-code`
**Pourquoi non:** Déjà dans comply.

### ❌ `no-duplicate-imports`
**Pourquoi non:** Déjà dans comply.

### ❌ `no-hardcoded-secret`
**Pourquoi non:** Déjà dans comply.

### ❌ `jsx-no-leaked-render`
**Pourquoi non:** Déjà dans comply.

### ❌ `drizzle-no-sql-raw-with-variable`
**Pourquoi non:** Déjà dans comply.

### ❌ `cognitive-complexity`
**Pourquoi non:** Déjà dans comply.

### ❌ `no-identical-functions`
**Pourquoi non:** Déjà dans comply.

### ❌ `ts-no-const-enum`
**Pourquoi non:** Déjà dans comply.

### ❌ `tanstack-query-array-key`
**Pourquoi non:** Déjà dans comply.

### ❌ `no-focused-test`
**Pourquoi non:** Déjà dans comply.

### ❌ `no-eval`
**Pourquoi non:** Déjà dans comply.

### ❌ `a11y-*` (33 règles)
**Pourquoi non:** Déjà dans comply pour JSX.

### ❌ `react-no-find-dom-node`
**Pourquoi non:** Déjà dans comply.

### ❌ `react-no-dangerously-set-inner-html`
**Pourquoi non:** Déjà dans comply.

### ❌ `no-unsanitized-method/property`
**Pourquoi non:** Déjà dans comply.

### ❌ Playwright (25+ règles)
**Pourquoi non:** La plupart sont déjà dans comply.

---

## 3.2 Nécessitent TypeScript type info

### ❌ `no-floating-promises` (ts-eslint version complète)
**Pourquoi non:** Comply n'a pas accès au type checker TS. La version AST-only couvre les cas évidents.

### ❌ `no-misused-promises`
**Pourquoi non:** Nécessite type info pour savoir si une valeur est une Promise.

### ❌ `no-unnecessary-condition`
**Pourquoi non:** Nécessite type info pour savoir si une condition est toujours true/false.

### ❌ `strict-boolean-expressions`
**Pourquoi non:** Nécessite type info.

### ❌ `restrict-template-expressions`
**Pourquoi non:** Nécessite type info pour savoir ce qui est interpolé.

### ❌ `await-thenable`
**Pourquoi non:** Nécessite type info pour savoir si la valeur est thenable.

### ❌ `no-unsafe-*` (family)
**Pourquoi non:** Toute la famille nécessite type info.

### ❌ `prefer-nullish-coalescing` (full)
**Pourquoi non:** La version complète nécessite type info.

### ❌ `functional-no-return-void`
**Pourquoi non:** Nécessite type info ou inférence complexe.

---

## 3.3 Trop spécifiques ou stylistic

### ❌ `eslint-plugin-es-x` (200+ règles)
**Pourquoi non:** Règles de compatibilité ES. Nécessitent une config browserslist/target que comply n'a pas. Très niche.

### ❌ `react-hook-form-*` (sauf destructuring-formstate)
**Pourquoi non:** Trop spécifiques à RHF. Faible base d'utilisateurs.

REVIEW: à faire

### ❌ `project-structure-*`
**Pourquoi non:** Nécessitent une config JSON complexe par projet. Pas de default sensé.

### ❌ `no-null`
**Pourquoi non:** Guerre undefined vs null. Trop opinionated pour être default.

REVIEW: à faire si il n'y a pas une règle existante

### ❌ `no-let`
**Pourquoi non:** Style FP strict. La plupart du code légitime utilise let.

REVIEW: à faire

### ❌ `no-foreach`
**Pourquoi non:** Préférence for-of vs forEach. Purement stylistic.

### ❌ `no-index-file`
**Pourquoi non:** Interdire index.ts est très opinionated.

REVIEW: à faire

### ❌ `filename-blocklist`
**Pourquoi non:** Quels noms bloquer ? Trop projet-spécifique.

### ❌ `top-level-function`
**Pourquoi non:** `function` vs `const fn = () =>` est une préférence.

REVIEW: à faire

### ❌ `clsx-*`
**Pourquoi non:** Optimisations mineures sur clsx/cn. Stylistic.

REVIEW: à faire

### ❌ `proper-arrows-*` (la plupart)
**Pourquoi non:** Contraintes sur arrow functions. Overlap et stylistic.

REVIEW: ça inclut quoi ?

### ❌ `toml-*`

**Pourquoi non:** Formatting TOML. Très niche.

REVIEW: REVIEW: à faire

### ❌ `markdown-*`
**Pourquoi non:** Formatting Markdown. Très niche.

REVIEW: à faire

### ❌ `vitest-prefer-lowercase-title`
**Pourquoi non:** Convention de naming. Stylistic.

### ❌ `xstate-event-names` / `xstate-state-names`
**Pourquoi non:** Naming conventions. Stylistic.

REVIEW: à faire

### ❌ `newline-before-return`
**Pourquoi non:** Purement formatting.

### ❌ `comment-syntax`
**Pourquoi non:** `//` vs `/* */`. Purement stylistic.

---

# RÉSUMÉ

| Catégorie | À implémenter | À considérer | À ne pas implémenter |
|-----------|---------------|--------------|----------------------|
| Sécurité | 14 | 0 | 0 |
| Async/Promises | 4 | 0 | 2 |
| Error Handling | 6 | 1 | 0 |
| Imports/Architecture | 8 | 3 | 5 |
| React | 8 | 0 | 5 |
| Database | 3 | 0 | 1 |
| Testing | 6 | 3 | 25+ |
| TypeScript | 5 | 0 | 9 |
| HTML/A11y | 10 | 0 | 33 |
| Performance | 1 | 0 | 0 |
| i18n | 3 | 0 | 0 |
| Code Quality | 5 | 4 | 10 |
| Immutabilité/FP | 0 | 4 | 3 |
| Tailwind | 0 | 5 | 0 |
| Zod | 0 | 3 | 0 |
| XState | 0 | 3 | 3 |
| Node/pkg.json | 0 | 3 | 0 |
| **TOTAL** | **~73** | **~29** | **~96** |
