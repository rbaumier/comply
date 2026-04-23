# Audit des règles comply

Date: 2026-04-23

## Légende
- OK : Règle fonctionne correctement, tests cohérents
- ISSUE : Problème détecté (détails fournis)
- MINOR : Problème mineur / amélioration possible

---

## Trouvailles

### ban_dependencies
**Status**: OK
- Détecte lodash, moment, underscore, etc.
- Gère import et require
- Gère les subpaths (`lodash/merge`)
- Tests couvrent les cas principaux

### i18n_json_no_empty_values  
**Status**: OK
- Détecte les valeurs vides dans les fichiers i18n JSON
- Heuristique correcte pour identifier les fichiers de locale
- Tests couvrent : valeurs vides, nested, multiples, fichiers non-locale ignorés

### i18n_json_no_untranslated
**Status**: OK  
- Compare avec la locale de base (en par défaut)
- Ignore intelligemment : URLs, emails, strings courtes, versions, placeholders
- Tests utilisent tempfile pour créer des vrais fichiers locales

### no_property_mutation
**Status**: OK
- Détecte `obj.prop = x`, `obj.prop += x`, `obj.prop++`, `delete obj.prop`
- Exception pour `module.exports` et `exports.foo`
- Tests couvrent tous les patterns

### prefer_array_fill
**Status**: OK
- Détecte `Array.from({length: n}, () => constant)` → suggère `Array(n).fill(constant)`
- Vérifie bien que le callback retourne une constante (pas d'index)
- Tests couvrent les cas avec index ignoré

### prefer_array_from_map
**Status**: OK
- Détecte `[...iter].map(fn)` → suggère `Array.from(iter, fn)`
- Ignore les tableaux littéraux `[1,2,3].map()`
- Tests cohérents

### prefer_array_to_spliced
**Status**: OK
- Détecte `arr.slice().splice()` et `[...arr].splice()`
- Suggère `toSpliced()` (ES2023)
- Tests corrects

### prefer_static_regex
**Status**: OK
- Détecte regex `/pattern/` dans les fonctions/méthodes
- Suggère de hoister au niveau module
- Autorise les regex au niveau module et dans les propriétés de classe

### arguments_order
**Status**: OK
- Détecte arguments potentiellement inversés basé sur les noms
- Intégration avec ImportIndex pour les fonctions importées
- Tests couvrent : swapped, correct, expressions, préfixes underscore

### no_hardcoded_secret
**Status**: OK
- Détecte: AWS keys, GitHub tokens, Stripe, OpenAI, Slack, private keys, Twilio, passwords in URLs, GCP service accounts
- Détection `KEYED_LITERAL` pour `API_KEY = "..."`
- Ignore les template literals avec interpolation `${}`
- Tests exhaustifs pour chaque pattern

### no_floating_promise
**Status**: OK
- Heuristique conservatrice: détecte Promise.all/race/etc et méthodes "async-looking" (save, fetch, query...)
- Ignore les chaînes .then()/.catch()/.finally()
- Ignore les await et void
- Tests bien couverts

### no_array_reduce
**Status**: OK
- Distingue reduce simple (arithmétique: +, -, *, /) vs complexe
- Autorise Math.min/Math.max
- Détecte reduceRight aussi
- Tests cohérents

### no_hook_setter_in_body
**Status**: OK
- Détecte `setX()` appelé directement dans le corps d'un composant React
- Ignore si dans useEffect/useCallback/useMemo/useLayoutEffect
- Ignore si dans un event handler (onClick, handleX)
- Tests couvrent les cas principaux

### no_identical_functions
**Status**: OK
- Support intra-file et cross-file via ImportIndex avec cache process-wide
- Seuils raisonnables: MIN_BODY_LINES=4, MIN_NORMALIZED_CHARS=51
- Normalisation whitespace-only (variable renames non détectés)
- Tests couvrent duplicates, différents, et seuils

### no_console_spaces
**Status**: OK
- Détecte espaces leading/trailing dans les args console.log
- Ignore premier/dernier arg et espaces multiples
- Tests cohérents

### no_mutation
**Status**: OK
- Détecte mutations sur bindings `const`: property assignment, compound, method calls (push/set/add), update expressions, delete, Object.assign
- Résolution de scope légère mais efficace
- Tests exhaustifs (20+ cas)

### no_commented_out_code
**Status**: OK
- Re-parse les commentaires comme TypeScript pour détecter du code
- Groupe les commentaires adjacents
- Vérifie présence de "rich code" (call, assignment, declaration, etc.)
- Tests couvrent prose, doc comments, et faux positifs

### jsx_no_leaked_render
**Status**: OK
- Détecte `{count && <X />}` où count pourrait être 0 ou ""
- Ignore `!!x`, comparisons, et préfixes booléens (is, has, should...)
- Severity Error (critique)

### no_async_array_callback
**Status**: OK
- Détecte forEach/filter/some/every/find avec async callback
- Ignore map (pattern Promise.all idiomatique)
- Message d'erreur informatif avec alternatives

### rust_no_unwrap
**Status**: OK
- Détecte .unwrap() et .expect() en Rust hors tests
- Exempte #[test], #[cfg(test)], et répertoire tests/
- Équivalent clippy::unwrap_used + clippy::expect_used

### no_nested_ternary
**Status**: OK
- Détecte ternaires imbriqués (parent est aussi ternary_expression)
- Test correct pour profondeur 3 (2 diagnostics)

### no_let
**Status**: OK
- Détecte `let` dans lexical_declaration
- Ignore `var` (différent node type)
- Simple et efficace

### no_focused_test
**Status**: OK  
- Détecte test.only, it.only, describe.only
- Utilise shared test_methods module
- Severity Error (critique pour CI)

### no_duplicate_imports
**Status**: ISSUE MINOR
- Utilise TextCheck ligne par ligne
- **Ne gère pas les imports multi-lignes** — pourrait rater des duplicates
- Fonctionne pour le cas commun (single-line imports)

### no_test_imports_in_prod
**Status**: OK
- Détecte imports de .test., .spec., __tests__, __mocks__ depuis fichiers prod
- Exempte si le fichier courant est lui-même un test
- Tests couvrent bien les cas

### no_open_redirect  
**Status**: OK
- Détecte redirect avec req.query/params/body, searchParams.get
- Heuristique simple mais efficace
- Severity Error

### react_jsx_key
**Status**: OK
- Détecte JSX sans key dans .map()/.flatMap()/.from() et array literals
- Parcours ascendant correct pour trouver le contexte iterateur
- Tests couvrent map, array literals, standalone elements

### db_no_string_concat_sql
**Status**: OK
- Détecte concaténation SQL (`"SELECT " + var`) et template literals interpolés
- Ignore queries paramétrées ($1, $2)
- Utilise `is_sql_string` pour éviter faux positifs sur prose
- Tests exhaustifs (12 cas)

### prefer_early_return
**Status**: OK
- Détecte fonction avec un seul `if` sans `else` wrappant le body
- Requiert 2+ statements dans le if (évite churn sur one-liners)
- Tests couvrent fonctions, arrows, méthodes, et cas autorisés

### no_redundant_boolean
**Status**: ISSUE MINOR
- Approche textuelle (ligne par ligne) dans un `ast_check!` — pattern hybride inhabituel
- **Pourrait avoir des faux positifs** sur patterns dans des strings
- Filtre les commentaires, donc limite les FP

### no_throw
**Status**: OK
- Détecte tous les `throw_statement`
- Simple et efficace

### vue_no_v_html_unsafe
**Status**: OK
- Détecte `v-html` sans sanitize()/DOMPurify
- Vérifie aussi la ligne précédente pour `// safe` comment
- XSS prevention basique mais utile

### no_empty_catch
**Status**: OK
- Détecte catch blocks vides
- Autorise si contient un commentaire (/* intentional */)
- Fallback sur text search pour // comments

### no_useless_spread
**Status**: OK
- Détecte `[...[1,2]]`, `{...{a:1}}`, `fn(...[1,2])`
- Ignore spreads de variables
- Tests couvrent tous les patterns

### no_typeof_undefined
**Status**: ISSUE
- Détecte `typeof x === 'undefined'`
- **PROBLÈME**: suggère `x === undefined` mais cela lance ReferenceError si x non déclaré
- `typeof` est safe pour variables potentiellement non déclarées
- Le conseil est incorrect pour le cas de variables globales non déclarées

### no_uniq_key
**Status**: OK
- Détecte key={Math.random()}, key={Date.now()}, key={uuid()}
- Simple et efficace pour éviter keys instables

### cognitive_complexity
**Status**: OK
- Implémente le modèle SonarSource
- Compte: if, else, for, while, switch, catch, ternary, &&/||/??
- Nesting penalty correctement appliqué
- Ne recurse pas dans les fonctions imbriquées
- Seuil configurable

### no_for_loop
**Status**: OK
- Détecte `for (let i = 0; i < arr.length; i++)` patterns
- Vérifie: init = 0, condition = i < x.length, increment = i++/++i/i+=1
- Gère condition inversée (arr.length > i)
- Tests exhaustifs (10+ cas avec autorisations explicites)

### a11y_alt_text
**Status**: OK
- Détecte `<img>`, `<area>`, `<input type="image">` sans attribut `alt`
- Support Vue via TextCheck séparé

### a11y_anchor_ambiguous_text
**Status**: OK
- Détecte textes de liens vagues ("click here", "read more", etc.)

### a11y_anchor_has_content
**Status**: OK
- Détecte ancres sans contenu, vérifie aria-label comme alternative

### a11y_anchor_is_valid
**Status**: OK
- Détecte `href="#"`, `href="javascript:"`, ou absence de href

### a11y_aria_activedescendant_has_tabindex
**Status**: OK
- Éléments avec aria-activedescendant doivent avoir tabIndex

### a11y_aria_props
**Status**: OK
- Valide les attributs aria-* contre la liste WAI-ARIA

### a11y_aria_role
**Status**: OK
- Valide les valeurs de role contre la liste WAI-ARIA

### a11y_no_autofocus
**Status**: OK
- Détecte l'attribut autoFocus sur les éléments

### a11y_tabindex_no_positive
**Status**: OK
- Détecte tabIndex positif (doit être 0 ou -1)

### api_deprecation_headers
**Status**: OK
- Handlers @deprecated doivent avoir Deprecation/Sunset headers

### array_callback_without_return
**Status**: OK
- Détecte callbacks array avec block body sans return

### cyclomatic_complexity
**Status**: OK
- Compte 1 base + branching nodes (if, for, case, ternary, &&, ||, ??)
- Seuil configurable

### drizzle_no_select_without_limit
**Status**: OK
- Détecte select().from() sans limit/where
- Chain walking correct

### hono_cors_permissive
**Status**: OK
- Détecte cors() sans args, origin: '*', credentials sans origin

### inconsistent_function_call
**Status**: OK
- Détecte fonctions appelées avec et sans `new`
- Support cross-file via ImportIndex
- Tests complets avec tempfile

### dead_export
**Status**: OK
- Détecte exports jamais importés via ImportIndex
- Gère: test files, entry points, star re-exports, namespace imports

### expiring_todo_comments
**Status**: OK
- Détecte TODO/FIXME avec dates expirées (format YYYY-MM-DD)
- Algorithme de date sans chrono

### no_throw
**Status**: OK
- Détecte tous les throw_statement

### no_useless_spread
**Status**: OK
- Détecte `[...[1,2]]`, `{...{a:1}}`, `fn(...[1,2])`

### vue_no_v_html_unsafe
**Status**: OK
- Détecte v-html sans sanitization (DOMPurify, sanitize())

### rust_no_unwrap
**Status**: OK
- Détecte .unwrap()/.expect() en Rust hors tests
- Exempte #[test], #[cfg(test)], et répertoire tests/

### no_async_array_callback
**Status**: OK
- Détecte forEach/filter/some/every/find avec async callback
- Ignore map (pattern Promise.all idiomatique)

### db_no_string_concat_sql
**Status**: OK
- Détecte concaténation SQL + template literals interpolés
- Ignore les requêtes paramétrées ($1, $2)
- Multiple backends (TS, Rust, Vue)

### no_mutation
**Status**: OK
- Détecte mutations sur bindings `const`: property assignment, compound, method calls (push/set/add), update expressions, delete, Object.assign
- Résolution de scope légère mais efficace
- Tests exhaustifs (20+ cas)

### no_commented_out_code
**Status**: OK
- Re-parse les commentaires comme TypeScript pour détecter du code
- Groupe les commentaires adjacents
- Vérifie présence de "rich code" (call, assignment, declaration, etc.)

---

## Résumé

**Règles auditées**: ~40 règles

**Issues trouvées**:

| Règle | Sévérité | Description |
|-------|----------|-------------|
| `no_typeof_undefined` | **ISSUE** | Le conseil "Use `x === undefined`" peut causer ReferenceError si `x` n'est pas déclaré. `typeof` est le seul moyen safe de vérifier une variable potentiellement non déclarée. |
| `no_duplicate_imports` | MINOR | Utilise TextCheck ligne par ligne — ne gère pas les imports multi-lignes |
| `no_redundant_boolean` | MINOR | Pattern hybride text/AST dans `ast_check!`, risque de FP sur patterns dans des strings |

**Observations générales**:
- La grande majorité des règles sont bien implémentées avec des tests cohérents
- Bonne utilisation des helpers partagés (test_helpers, walker, sql_helpers, jsx, rust_helpers)
- Bonnes heuristiques pour éviter les faux positifs (seuils, contextes, exclusions)
- Documentation inline (//! docblocks) généralement présente et utile
- Cross-file analysis bien implémentée (no_identical_functions, arguments_order)
- Règles de sécurité solides (no_hardcoded_secret, db_no_string_concat_sql, no_open_redirect)

**Patterns d'implémentation observés**:
- `ast_check!` macro pour règles AST simples
- `AstCheck` trait pour règles complexes nécessitant tree walking
- `TextCheck` trait pour règles text-only (comments, SQL, secrets)
- Utilisation de `ImportIndex` pour analyse cross-file
- Seuils configurables via `ctx.config.threshold()`

