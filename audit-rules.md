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

### no_console_spaces
**Status**: OK
- Détecte espaces leading/trailing dans console.log args
- Ignore premier/dernier arg et espaces multiples

### no_uniq_key
**Status**: OK
- Détecte key={Math.random()}, Date.now(), uuid(), nanoid()

### no_test_imports_in_prod
**Status**: OK
- Détecte imports de .test., .spec., __tests__, __mocks__ depuis fichiers prod

### i18n_json_identical_placeholders
**Status**: OK
- Compare les placeholders ICU ({name}, {count}) entre locales
- Détecte missing/extra placeholders
- Tolère ordre différent

### data_clumps
**Status**: OK
- Détecte 3+ paramètres identiques dans plusieurs fonctions
- Support cross-file via ImportIndex

### halstead_complexity
**Status**: OK
- Calcule les métriques de Halstead (volume, difficulté, effort)
- Seuils configurables

### nested_control_flow
**Status**: OK
- Compte la profondeur des structures de contrôle (if, for, while, etc.)
- Gère correctement les cascades else if
- Seuil configurable

### max_dependencies
**Status**: OK
- Compte les imports uniques par fichier
- Déduplique les imports du même module
- Note: même limitation que no_duplicate_imports (ligne par ligne)

### no_dangerously_set_inner_html
**Status**: OK
- Détecte l'attribut dangerouslySetInnerHTML dans JSX

### no_async_constructor
**Status**: OK
- Détecte les constructeurs async

### no_catch_without_use
**Status**: OK
- Détecte les bindings catch non utilisés
- Gère correctement les destructuring et bare catch

### no_collapsible_if
**Status**: OK
- Détecte les if imbriqués qui peuvent être fusionnés avec &&

### avoid_barrel_files
**Status**: OK
- Détecte les fichiers barrel (3+ re-exports sans autre code)

### explicit_length_check
**Status**: OK
- Détecte les coercions implicites de .length/.size
- Pattern text-based avec bonne heuristique

### id_length
**Status**: OK
- Détecte les identifiants trop courts
- Exceptions configurables et patterns regex

### boolean_naming
**Status**: OK
- Vérifie les préfixes prédicat (is/has/should/can) sur booléens
- Détecte les formes négatives (isNotReady)

### a11y_aria_unsupported_elements
**Status**: OK
- Détecte aria-*/role sur éléments non supportés (meta, html, script, style, head, title, link, base)
- Tests couvrent meta avec aria-hidden, script avec role, div autorisé

### a11y_autocomplete_valid
**Status**: OK
- Valide les valeurs autocomplete contre la liste WAI-ARIA complète
- Gère les tokens multiples ("shipping street-address") et section- préfixes
- Tests couvrent valeurs invalides, email valide, off valide

### a11y_click_events_have_key_events
**Status**: OK
- Détecte onClick sans onKeyDown/onKeyUp/onKeyPress
- Tests couvrent multiline, différents handlers clavier

### a11y_control_has_associated_label
**Status**: OK
- Détecte button/input/select/textarea sans aria-label/aria-labelledby
- Exempte input type="hidden"
- Vérifie le contenu texte pour les boutons non self-closing
- Tests complets

### a11y_heading_has_content
**Status**: OK
- Détecte h1-h6 self-closing ou vides
- Vérifie le contenu texte et les enfants JSX
- Severity Error (critique pour a11y)

### api_first
**Status**: OK
- Détecte routes (.get/.post/.put/.delete) sans définition de schéma
- Vérifie présence de z, createRoute, openapi, schema, zodValidator
- Heuristique textuelle simple mais efficace

### api_import_from_public_index
**Status**: OK
- Détecte imports cross-feature (2+ `../`) qui ciblent des fichiers internes
- Autorise les imports vers l'index de feature (`../../users`)
- Whitelist: `types`, `utils` comme leaves partagés communs
- Tests couvrent deep cross-feature, index autorisé, single parent autorisé

### api_list_requires_pagination
**Status**: OK
- Détecte `export async function GET` / `export const GET` sans termes de pagination
- Vérifie présence de: limit, cursor, page, offset, pageSize, per_page
- TextCheck - heuristique coarse mais utile pour éviter les endpoints unbounded

### api_no_array_root_response
**Status**: OK
- Détecte `Response.json([`, `res.json([`, `c.json([`, `return json([`
- Suggère `{ data: [...] }` pour extensibilité
- TextCheck simple, ignore les commentaires
- Tests couvrent array flaggé, object autorisé

### api_no_boolean_field_in_response
**Status**: OK
- Détecte champs `boolean` dans interfaces/types avec suffixes Response/DTO/Payload/Reply/Result/Body
- Suggère string-union/enum pour extensibilité API
- Ignore `boolean | null`, `boolean[]` (types plus riches)
- Tests couvrent interface, type alias, multiples champs, non-response autorisé

### arrow_this_in_function
**Status**: OK
- Détecte `this` dans arrow function sans fonction régulière englobante
- Parcours ascendant correct: function_declaration, method_definition, etc. bindent `this`
- Tests couvrent top-level arrow, nested arrows, class method autorisé

### assertions_in_tests
**Status**: OK
- Détecte tests `it`/`test` sans assertion (expect, assert, .should, .toBe, etc.)
- Ne s'active que sur fichiers .test./.spec./__tests__
- Récursif sur le body mais ne descend pas dans les tests imbriqués
- Severity Error - critique pour CI
- Tests complets avec différents patterns d'assertion

### audit_log_required_fields
**Status**: OK
- Détecte appels auditLog/audit.log sans champs requis
- Champs requis avec alias: userId/actorId, timestamp/createdAt, action/event
- Vérifie les objets littéraux passés en argument
- Tests couvrent chaque champ manquant et les alias acceptés

### auth_on_mutation
**Status**: OK
- Détecte routes mutation (POST/PUT/DELETE/PATCH) sans référence auth
- Vérifie présence de: auth, token, session, middleware, guard, protect, verify
- Recherche case-insensitive dans tout le call expression
- GET autorisé sans auth (lecture publique)
- Tests couvrent middleware, verify inline, absence flaggée

### avoid_importing_barrel_files
**Status**: OK
- Détecte imports relatifs vers barrels: `/index`, `/index.ts`, trailing slash, `.`, `..`
- Ignore les imports de packages npm (tree-shakers les gèrent)
- Tests couvrent: index explicite, extensions, trailing slash, `.`/`..`, fichier direct autorisé

### avoid_re_export_all
**Status**: OK
- Détecte `export * from '...'` (star re-exports)
- Autorise `export * as ns from '...'` (namespace explicite)
- Autorise `export { foo } from '...'` (named re-exports)
- Utilise AstCheck avec walker pour parcourir l'arbre

### banned_comment_words
**Status**: OK
- Détecte mots dismissifs: obviously, simply, just, basically, clearly, trivially
- Vérifie les word boundaries (évite FP sur "simplify")
- Ne s'active que dans les commentaires (// ou /*)
- Un diagnostic par ligne max
- Severity Error - ces mots cachent la complexité

### better_auth_middleware_requires_headers
**Status**: OK
- Ne s'active que sur middleware.ts/.tsx/.js
- Détecte `getSession()` sans paramètre `headers` dans l'objet argument
- Severity Error - session lookup échoue sinon dans Next.js
- Tests couvrent: no args, sans headers, avec headers, fichiers non-middleware ignorés

### block_scope_case
**Status**: OK
- Détecte let/const/class dans case sans block wrapper `{ }`
- Ces déclarations leakent dans les cases adjacents (TDZ errors)
- Un diagnostic par case max
- Tests couvrent const, let, class, case avec block autorisé

### catch_error_name
**Status**: OK
- Détecte noms de catch parameter non standard (e, err, ex, exception)
- Suggère `error` comme nom canonique
- Autorise: `error`, `_`, noms finissant par `Error`/`error`, `innerError`
- Ignore les bare catch `catch {}` et destructuring `catch ({ message })`

### comma_or_logical_or_case
**Status**: OK
- Détecte `case 1, 2:` (sequence_expression) et `case 1 || 2:` (binary_expression)
- Severity Error - ces patterns ne fonctionnent pas comme attendu en JS
- Suggère fall-through pattern avec cases séparés
- Tests couvrent comma, ||, case simple autorisé, fall-through autorisé

### comment_paraphrases_code
**Status**: OK
- Détecte commentaires courts qui paraphrasent le nom de fonction
- Tokenize camelCase/snake_case et compare avec tokens du commentaire
- Seuils: max_comment_tokens=6, overlap_threshold=0.8 (80%)
- Ignore JSDoc (géré par autres règles)
- Filtre les stop-words (a, the, to, etc.)
- Tests couvrent paraphrase flaggée, commentaire informatif autorisé

### comment_prose_quality
**Status**: OK
- Détecte: weasel words (basically, actually, etc.), passive voice (is used, was called), lexical illusions (mots répétés entre lignes)
- TextCheck sur les lignes de commentaires
- Gère //, /*, et continuations de block comments (*)
- Gère les doc-comments Rust (//!, ///)
- Un diagnostic par type par ligne

### consistent_date_clone
**Status**: OK
- Détecte `new Date(d.getTime())` et `new Date(d.valueOf())`
- Suggère `new Date(d)` directement
- Vérifie que l'appel interne n'a pas d'arguments
- Autorise `new Date(Date.now())` et `new Date(number)`

### consistent_destructuring
**Status**: OK
- Détecte `user.age` après `const { name } = user`
- Suggère d'ajouter `age` à la destructuration
- Skip intelligemment: computed access `user[key]`, method calls `user.greet()`, assignments `user.age = 5`, nested `user.address.city`
- Skip si rest element présent `{ name, ...rest }`
- Deux passes: collecte des destructurations, puis détection des accès

### consistent_empty_array_spread
**Status**: OK
- Détecte `[...condition ? ['a'] : []]` sans parenthèses
- Le ternaire non parenthésé peut être confus
- Suggère `[...(condition ? ['a'] : [])]`
- Simple vérification: spread_element contient directement ternary_expression

### consistent_template_literal_escape
**Status**: OK
- Détecte `$\{` et `\$\{` dans template literals (mauvais escapes)
- Le bon escape est `\${`
- Gère correctement les backslashes précédents (odd count)
- Skip les vraies interpolations `${...}` avec tracking de depth
- Tests couvrent les patterns corrects et incorrects

### custom_error_definition
**Status**: OK
- Détecte `this.name = 'X'` dans constructeurs de classes Error
- Suggère class field `name = 'ClassName'`
- Détecte aussi `this.message = ...` - suggère passer à super()
- Vérifie que la classe extends un nom finissant par "Error"
- Tests couvrent: this.name, this.message, les deux, class field autorisé

### db_no_n_plus_one
**Status**: OK
- Détecte `await db.query()` dans les boucles (for, while, forEach, map)
- Severity Error - problème de performance critique
- Reconnaît: query, execute, findFirst, findMany, create, update, delete
- Reconnaît: db.*, prisma.*, drizzle.*
- Message suggère JOIN ou WHERE IN

### de_morgan_simplify
**Status**: OK
- Détecte `!(a && b)` et `!(a || b)` 
- Suggère application de la loi de De Morgan: `!a || !b` ou `!a && !b`
- Vérifie que l'argument de ! est parenthesized_expression contenant binary_expression
- Ignore les comparisons `!(a === b)`

### detect_dangerous_redirects
**Status**: OK
- Détecte `res.redirect(req.query.x)` - open redirect vulnerability
- Vérifie que l'argument est rooted sur `req` (req.query, req.body, req.params)
- Gère les deux formes: `(url)` et `(status, url)`
- Severity Error - faille de sécurité

### elseif_without_else
**Status**: OK
- Détecte chaînes `if/else if` sans `else` final
- Parcourt la chaîne depuis le if racine
- Ignore les if simples sans else if
- Diagnostic sur le dernier else if de la chaîne

### error_message
**Status**: OK
- Détecte `new Error()` sans message ou avec message invalide
- Gère tous les built-in errors (Error, TypeError, etc.)
- Gère AggregateError (message à index 1) et SuppressedError (index 2)
- Détecte: absence de message, string vide, array/object/number/boolean
- Bail si spread avant l'argument message
- Tests exhaustifs (15+ cas)

### error_message_is_remediation
**Status**: OK
- Détecte messages d'erreur trop vagues
- Critères: longueur < 15 chars OU absence de verbe
- Liste de verbes d'action (is, are, failed, cannot, missing, etc.)
- Gère string et template_string
- Suggère de décrire le problème et la solution

### error_without_cause
**Status**: OK
- Détecte `new Error(e.message)` sans `{ cause: e }`
- Signal de wrap: accès à `.message` dans les args
- Ignore les erreurs fraîches avec littéral
- Severity Error - perte de stack trace importante
- Gère tous les built-in Error types

### escape_case
**Status**: OK
- Détecte hex lowercase dans escape sequences: `\xff`, `ÿ`, `\u{ff}`
- Suggère uppercase: `\xFF`, `ÿ`, `\u{FF}`
- Regex pour matcher les escape sequences
- Vérifie que le backslash n'est pas lui-même escaped (count pair)
- Calcul correct de position dans les strings multilignes

### exception_use_error_cause
**Status**: OK
- Plus strict que error_without_cause: détecte dans catch_clause
- `throw new Error('msg')` dans catch doit avoir `{ cause: e }`
- Stop aux frontières de fonction (catch externe ne compte pas)
- Ignore `new Error()` sans args (placeholder)
- Gère tous les built-in Error types

### expression_complexity
**Status**: OK
- Compte les opérateurs logiques/conditionnels par ligne: &&, ||, ??, ?
- Seuil configurable (default 4)
- Ignore `?.` (optional chaining)
- Ignore les commentaires
- Suggère d'extraire vers des variables nommées

### factory_di_shape
**Status**: OK
- Détecte `export function create*` avec 3+ paramètres séparés
- Suggère un seul objet deps: `createService({ db, cache, logger })`
- Ignore si params déjà destructurés `{ db, cache }`
- Pattern textuel simple mais efficace

### file_name_differ_from_class
**Status**: OK
- Fichier avec un seul export class/function doit matcher le nom de fichier
- Comparaison case-insensitive: PascalCase, camelCase, kebab-case, snake_case
- Skip: index, types, constants, utils (barrels conventionnels)
- Skip: exports multiples, variables, re-exports, anonymous
- Gère `.d.ts` correctement (strip toutes les extensions)
- Tests exhaustifs (20+ cas)

### for_loop_increment_sign
**Status**: OK
- Détecte `for (i < 10; i--)` et `for (i > 0; i++)`
- Boucle infinie ou jamais exécutée
- Severity Error - bug logique probable
- Pattern textuel simple: parse les 3 parties du for

### function_inside_loop
**Status**: OK
- Détecte function/arrow déclarées dans les boucles (for, while, do)
- Crée un nouvel objet fonction à chaque itération
- Stop aux frontières de fonction (nested functions OK)
- Tests couvrent for, while, fonction externe autorisée

### function_return_type
**Status**: OK
- Détecte fonctions retournant des types inconsistants
- Inférence de type: number, string, boolean, null, undefined, array, object
- Ignore les patterns nullable courants (value | null, value | undefined)
- Ne descend pas dans les fonctions imbriquées

### generator_without_yield
**Status**: OK
- Détecte `function*` sans `yield` dans le body
- Parse textuel avec tracking de depth pour les braces
- Vérifie les word boundaries pour "yield"
- Ignore les commentaires

### god_module
**Status**: OK
- Détecte modules importés par trop de fichiers (cross-file via ImportIndex)
- Seuils: min_importers (10) ET threshold_percent (30%)
- Évite faux positifs sur petits projets avec min_importers absolu
- TextCheck car utilise uniquement l'index, pas le contenu

### index_of_compare_to_positive
**Status**: OK
- Détecte `.indexOf(x) > 0` et `.indexOf(x) < 1`
- Bug: rate l'index 0 (première position)
- Suggère `>= 0` ou `!== -1`
- Severity Error - bug logique probable

### intermediate_variables
**Status**: OK
- Détecte conditions `if` avec 3+ opérateurs logiques (&&, ||, ??)
- Seuil configurable (min_ops)
- Stop aux frontières de fonction (lambdas dans condition ignorées)
- Suggère d'extraire vers des variables nommées

### inverted_assertion_arguments
**Status**: OK
- Détecte `expect(42).toBe(result)` - arguments inversés
- Ne s'active que sur fichiers test (.test., .spec., __tests__)
- Vérifie expect(literal).toBe/toEqual(variable)
- Suggère de mettre le littéral dans toBe/toEqual

### layer_import_boundary
**Status**: OK
- Enforce architecture hexagonale (Clean Architecture)
- domain/ ne peut pas importer de infrastructure/ ou application/
- application/ ne peut pas importer de infrastructure/
- Détecte les couches via segments de path
- TextCheck ligne par ligne sur les imports

### max_call_chain_depth
**Status**: OK
- Détecte appels imbriqués profonds: `a(b(c(d(e(x)))))`
- Seuil configurable (default 4)
- Compte récursif de la profondeur max dans les arguments
- Ignore les method chains `a.b().c().d()`
- Un seul diagnostic par chaîne (outermost call seulement)

### max_union_size
**Status**: OK
- Détecte union types avec trop de membres: `A | B | C | D | E | F`
- Seuil configurable (default 5)
- Compte récursif car tree-sitter parse en arbre left-recursive
- Ne flagge que le union_type le plus externe

### new_for_builtins
**Status**: OK
- Enforce `new` pour: Map, Set, Promise, Date, etc.
- Interdit `new` pour: Symbol, BigInt (pas des constructeurs)
- Severity Error - erreur runtime
- Tests exhaustifs pour les deux directions

### no_abbreviated_names
**Status**: OK
- Détecte abréviations obscures: acct, usr, btn, pwd, cnt, desc, addr
- N'inclut PAS les idiomes courants: ctx, idx, cfg, err, req, res, etc.
- Split camelCase/snake_case pour extraire les mots
- Suggère le mot complet

### no_accessor_recursion
**Status**: OK
- Détecte `get foo() { return this.foo; }` - récursion infinie
- Détecte `set foo(v) { this.foo = v; }` - récursion infinie
- Gère les arrow functions (héritent `this`)
- Stop aux frontières de fonctions régulières
- Suggère backing field `_foo` ou WeakMap

### no_anonymous_default_export
**Status**: OK
- Détecte `export default function() {}` et `export default class {}`
- Exports anonymes = stack traces illisibles, refactoring difficile
- Autorise `export default function myFn() {}` et `export default myVar`

### no_auth_token_in_localstorage
**Status**: OK
- Détecte `localStorage.setItem('token', ...)` et variantes (jwt, authToken, accessToken, refreshToken, session)
- XSS peut voler les tokens localStorage — httpOnly cookies sont plus sûrs
- Gère bracket notation `localStorage['setItem']('jwt', ...)`
- Severity Error - faille de sécurité
- Tests couvrent: token, jwt, authToken, accessToken, bracket notation, clés sûres autorisées

### no_await_in_promise_methods
**Status**: OK
- Détecte `await` dans les arrays passés à Promise.all/race/allSettled/any
- `await` dans array sérialise les calls, annulant le bénéfice de Promise.all
- Vérifie: spread_element avec await, ou éléments directs avec await
- Severity Warning
- Tests couvrent: await in array, sans await autorisé, spread avec await

### no_bidi_characters
**Status**: OK
- Détecte les caractères Unicode bidirectionnels (trojan-source attack)
- Liste: U+202A-202E, U+2066-2069, U+200E-200F (LRE, RLE, PDF, etc.)
- TextCheck - scan ligne par ligne pour ces codepoints
- Severity Error - faille de sécurité (code malicieux invisible)
- Tests couvrent: RLO, LRI, LRE dans strings, code clean autorisé

### no_bitwise_in_boolean
**Status**: OK
- Détecte &, |, ^, ~ dans les conditions if/while (probable typo pour &&/||)
- Récursif pour détecter bitwise imbriqués dans la condition
- Autorise bitwise hors contexte boolean (masques légitimes)
- Tests couvrent: &, |, ^ dans if/while, && et || autorisés, bitwise hors condition autorisé

### no_boolean_flag_param
**Status**: OK
- Détecte paramètres typés `: boolean` dans les fonctions
- Philosophie: `fn(x, isUrgent: boolean)` → splitter en deux fonctions nommées
- Message détaillé expliquant pourquoi (testabilité, lisibilité call-site)
- Vérifie que le param est dans `formal_parameters` (pas destructuring)
- Severity Error - design smell important
- Tests couvrent: function, arrow, multiples params boolean, variable boolean autorisée

### no_built_in_override
**Status**: OK
- Détecte `const Array = []`, `let Object = {}`, etc.
- Liste: Array, Object, String, Map, Set, Promise, JSON, Math, undefined, NaN, Infinity
- Vérifie qu'il y a une valeur assignée (pas juste déclaration)
- Severity Error - bug quasi-certain
- Tests couvrent: const, let, undefined override, usage normal autorisé

### no_case_label_in_switch
**Status**: OK
- Détecte `labeled_statement` dans switch (confusion `foo:` vs `case foo:`)
- Erreur fréquente: oublier `case` et écrire juste `label:` qui est un JS label
- Parcours ascendant pour vérifier si dans switch_body
- Severity Error - bug quasi-certain
- Tests couvrent: label dans switch, multiples labels, case/default autorisés, labels hors switch autorisés

### no_catch_log_rethrow
**Status**: OK
- Détecte `catch (e) { log(e); throw e; }` - pattern inutile
- Vérifie exactement 2 statements: expression_statement(log) + throw_statement
- Liste de loggers: console.*, logger.*, Sentry.captureException, Rollbar, Bugsnag
- Autorise si wrap avec contexte, si pas de rethrow, si travail supplémentaire
- Severity Warning

### no_class_inheritance
**Status**: OK
- Détecte `class Foo extends Bar` - préfère composition
- Exception: extends *Error (Error, CustomError, TaggedError, etc.)
- Vérifie via class_heritage > extends_clause
- Severity Warning
- Tests couvrent: extends flaggé, sans extends autorisé, extends Error autorisé

### no_clear_text_protocol
**Status**: OK
- Détecte `http://`, `ftp://`, `telnet://` dans les strings
- Exceptions intelligentes: localhost, 127.0.0.1, préfixes nus (`"http://"` comme needle de recherche)
- Utilise helper `is_clear_text_url()` partagé
- Severity Error - faille de sécurité
- Tests couvrent: http flaggé, https autorisé, localhost autorisé, prefix detection ignoré

### no_collection_size_mischeck
**Status**: OK
- Détecte `.length >= 0` (toujours vrai) et `.length < 0` (toujours faux)
- Gère aussi `.size` (Map, Set)
- Severity Error - bug logique
- Tests couvrent: >= 0 flaggé, < 0 flaggé, > 0 autorisé, === 0 autorisé

### no_common_grab_bag
**Status**: OK
- Détecte fichiers nommés `common.ts`, `utils.ts`, `helpers.ts`, `shared.ts`, `misc.ts`
- Ces noms attirent du code non-relié - forcer un nom descriptif
- TextCheck basé uniquement sur le nom de fichier
- Severity Warning
- Tests couvrent: utils.ts flaggé, common.js flaggé, noms significatifs autorisés

### no_conditional_async_return
**Status**: OK
- Détecte fonctions mélangeant retours sync et promise (`T | Promise<T>`)
- Classifie les returns: .then()/.catch(), Promise.resolve/reject/all, ou sync
- Skip les fonctions `async` (wrapping automatique)
- Ne descend pas dans les fonctions imbriquées
- Severity Warning
- Tests couvrent: mix Promise.resolve+sync flaggé, tout sync autorisé, async autorisé

### no_conditional_tests
**Status**: OK
- Détecte `test`/`describe`/`it` dans if/ternary/switch_case
- Rend la suite non-déterministe - préférer .skip/.skipIf
- Parcours ascendant pour vérifier contexte conditionnel
- Gère test.each, describe.only etc.
- Severity Warning
- Tests couvrent: test dans if flaggé, describe dans ternary flaggé, top-level autorisé

### no_confidential_logging
**Status**: OK
- Détecte console.log/logger.* avec données sensibles dans les args
- Mots-clés: password, secret, token, apikey, authorization, credential, ssn, creditcard
- Check case-insensitive sur tout le texte des arguments
- Severity Error - faille de sécurité
- Tests couvrent: password flaggé, token flaggé, logging normal autorisé

### no_constructor_side_effects
**Status**: OK
- Détecte `new X();` comme expression_statement (sans assignation)
- Constructeurs ne devraient pas avoir d'effets secondaires
- Autorise: assignation, return, throw
- Severity Warning
- Tests couvrent: standalone flaggé, assigned autorisé, returned autorisé, thrown autorisé

### no_default_export
**Status**: OK
- Détecte `export default ...`
- Préfère les named exports (refactoring, tree-shaking, IDE support)
- Autorise re-export default `export { default } from '...'`
- Severity Warning
- Tests couvrent: default function/class/expression flaggés, named exports autorisés

### no_delete
**Status**: OK
- Détecte l'opérateur `delete obj.prop`
- Delete mute l'objet et déoptimise les shapes JS
- Suggère de créer un nouvel objet sans la propriété
- Severity Warning
- Tests couvrent: delete prop flaggé, delete computed flaggé, rest destructuring autorisé

### no_deprecated_api
**Status**: OK
- Détecte APIs Node.js dépréciées: new Buffer(), url.parse(), require('domain'), fs.exists(), util.isArray()
- Tables de patterns pour requires, member calls, et member access
- Messages d'erreur spécifiques avec alternative suggérée
- Severity Warning
- Tests couvrent: new Buffer flaggé, url.parse flaggé, Buffer.from autorisé, new URL autorisé

### no_deprecated_cipher
**Status**: OK
- Détecte `createCipher()` (dépréciée) mais pas `createCipheriv()`
- crypto.createCipher utilise un IV dérivé non-sûr
- Gère appels qualifiés et non-qualifiés
- Severity Error - faille crypto
- Tests couvrent: createCipher flaggé, createCipheriv autorisé

### no_disable_mustache_escape
**Status**: OK
- Détecte désactivation de l'escaping HTML dans les template engines
- Patterns: escapeMarkup=false, escape=false, noEscape=true
- Gère assignments et object properties
- Severity Error - prévention XSS
- Tests couvrent: assignment flaggé, property flaggé, escape enabled autorisé

### no_document_cookie
**Status**: OK
- Détecte tout accès à `document.cookie` (lecture et écriture)
- Suggère d'utiliser une lib cookie à la place
- Severity Warning
- Tests couvrent: lecture flaggée, écriture flaggée, autres propriétés autorisées

### no_document_domain
**Status**: OK
- Détecte `document.domain = ...` (assignment seulement)
- Affaiblit la same-origin policy - vecteur de sécurité
- Suggère postMessage ou CORS
- Autorise la lecture de document.domain
- Severity Error

### no_document_write
**Status**: OK
- Détecte `document.write()` et `document.writeln()`
- Vecteur XSS et réouvre le document
- Suggère DOM APIs (appendChild, innerHTML sanitisé)
- Severity Error
- Tests couvrent: write flaggé, writeln flaggé, createElement autorisé

### no_done_callback
**Status**: OK
- Détecte `test('x', (done) => {...})` pattern legacy Mocha/Jest
- Vérifie test/it + modifiers (only, skip) avec callback ayant un paramètre
- Suggère async/await
- Severity Warning
- Tests couvrent: done arrow flaggé, done function flaggé, test.only flaggé, async autorisé

### no_double_cast
**Status**: OK
- Détecte `x as unknown as T` double casts
- Cache les types mal alignés derrière deux casts
- Suggère d'aligner les interfaces ou valider avec type guard/Zod
- Severity Error
- Tests couvrent: as unknown as T flaggé, as any as T flaggé, single cast autorisé

### no_duplicate_in_composite
**Status**: OK
- Détecte duplicates dans union `A | A` ou intersection `A & A`
- Un diagnostic par composite max
- Severity Warning
- Tests couvrent: duplicate union flaggé, duplicate intersection flaggé, unique autorisé

### no_duplicate_string
**Status**: OK
- Détecte strings répétées 3+ fois (longueur min pour éviter bruit)
- Un diagnostic par occurrence au-delà de la 2ème
- Ignore commentaires, template inner content
- Implémentation partagée entre backends via `collect_diagnostics`
- Tests couvrent: 3 occurrences flaggé, 2 autorisé, strings courtes ignorées

### no_duplicated_branches
**Status**: OK
- Détecte if/else branches avec bodies identiques
- Parcours récursif des chaînes else-if
- Normalise le texte des bodies (strip braces, trim)
- Un diagnostic par branche dupliquée (pas par paire)
- Severity Warning
- Tests couvrent: if/else identiques flaggé, else-if chain flaggée, différents autorisés

### no_dynamic_template
**Status**: OK
- Détecte innerHTML, outerHTML, document.write, insertAdjacentHTML, createContextualFragment, setHTMLUnsafe, location.href=
- Couvre aussi dangerouslySetInnerHTML en JSX
- Suggère safe DOM APIs ou framework escaping
- Severity Warning
- Tests couvrent: innerHTML flaggé, document.write flaggé, textContent autorisé

### no_ecb_mode
**Status**: OK
- Détecte mode ECB dans les strings crypto (aes-128-ecb, aes-256-ecb)
- ECB ne diffuse pas les patterns - chaque bloc est chiffré indépendamment
- Suggère CBC, CTR ou GCM
- Severity Error - faille crypto
- Tests couvrent: aes-128-ecb flaggé, cbc autorisé, gcm autorisé

### no_electron_node_integration
**Status**: OK
- Détecte `nodeIntegration: true` dans webPreferences de BrowserWindow/BrowserView
- Couvre aussi nodeIntegrationInWorker et nodeIntegrationInSubFrames
- Gère as expressions, namespaced constructors (electron.BrowserWindow)
- Severity Error - faille sécurité Electron majeure
- Tests exhaustifs (10+ cas)

### no_element_overwrite
**Status**: OK
- Détecte écritures consécutives au même index: `arr[0] = 1; arr[0] = 2;`
- Détecte aussi `.set()` avec même clé
- Compare les targets de 2 expression_statements adjacents
- Severity Error - première écriture est morte
- Tests couvrent: bracket writes flaggé, map.set flaggé, indices différents autorisés

### no_empty_file
**Status**: OK
- TextCheck - détecte fichiers sans contenu significatif
- Ignore: whitespace, comments, "use strict", triple-slash directives
- Severity Warning
- Tests couvrent: vide flaggé, whitespace only flaggé, comments only flaggé, code autorisé

### no_empty_test_file
**Status**: OK
- TextCheck - détecte fichiers .test./.spec./__tests__ sans assertions
- Cherche markers: test(, it(, describe(, expect(
- Ne s'active que sur fichiers test
- Severity Error - tests sans assertions sont inutiles
- Tests couvrent: test file vide flaggé, avec tests autorisé, non-test file autorisé

### no_enum
**Status**: OK
- Détecte toute `enum` declaration TypeScript
- Enums émettent du code runtime et ne narrowent pas proprement
- Suggère `as const satisfies Record` ou discriminated union
- Severity Error
- Tests couvrent: enum flaggé, const enum flaggé, as const autorisé

### no_equals_in_for_termination
**Status**: OK
- Détecte `==`/`===` dans condition de for loop
- Peut rater la terminaison si off-by-one
- Suggère `<`, `<=`, `>`, `>=`
- Ignore `!==` et `!=`
- Severity Warning
- Tests couvrent: === flaggé, == flaggé, < autorisé, !== autorisé

### no_error_details_in_response
**Status**: OK
- Détecte `err.message`/`err.stack` dans res.json, Response.json, etc.
- Fuite d'informations internes au client
- Reconnaît variables err/error/e
- Severity Error - faille de sécurité
- Tests couvrent: err.message flaggé, error.stack flaggé, message générique autorisé

### no_eval
**Status**: OK
- Détecte tout appel à `eval()`
- Vecteur d'injection de code arbitraire
- Severity Error - faille de sécurité majeure
- Tests couvrent: eval flaggé, evaluate autorisé

### no_extra_arguments
**Status**: OK
- Détecte appels avec plus d'arguments que de paramètres définis
- Collecte les signatures locales, puis vérifie les call sites
- Gère rest params (...rest) - pas de warning si présent
- Gère function declarations et arrow functions
- Severity Warning
- Tests couvrent: extra args flaggé, correct autorisé, rest params autorisé

### no_fire_event
**Status**: OK
- Détecte `fireEvent.*` dans les fichiers test
- Préfère userEvent qui simule mieux le comportement utilisateur
- Ne s'active que sur .test./.spec./__tests__
- Severity Warning
- Tests couvrent: fireEvent flaggé en test, userEvent autorisé, non-test autorisé

### no_for_in_iterable
**Status**: OK
- Détecte `for...in` sur arrays/iterables (heuristique par nom)
- for...in itère sur les clés string, pas les valeurs
- Heuristique: arr, list, items, elements, array, values, etc.
- Suggère for...of
- Severity Error
- Tests couvrent: myArray flaggé, itemsList flaggé, obj autorisé, for...of autorisé

### no_full_import
**Status**: OK
- Détecte `import _ from 'lodash'` et `import * as _ from 'lodash'`
- Libraries lourdes: lodash, underscore, ramda
- Casse tree-shaking, inclut toute la lib
- Autorise named imports et sub-path imports
- Severity Warning
- Tests couvrent: default flaggé, namespace flaggé, named autorisé, sub-path autorisé

### no_function_declaration_in_block
**Status**: OK
- Détecte `function foo() {}` dans if/for/while/switch
- Comportement de hoisting inconsistant entre strict et sloppy mode
- Parcours ascendant pour vérifier control-flow parents
- Autorise arrows et function expressions
- Severity Error
- Tests couvrent: function in if flaggé, function in for flaggé, top-level autorisé

### no_function_overloads
**Status**: OK
- Détecte les overload signatures TypeScript (2+ signatures même nom)
- Overloads ne contraignent pas l'implémentation - checking fait contre la dernière seule
- Suggère union types ou générics
- Severity Warning
- Tests couvrent: overloads flaggés (2 diagnostics), single signature autorisé

### no_generic_names
**Status**: OK
- Deux modes: mots interdits (info, temp, result, obj, val, foo, bar) + préfixes interdits (process, data, do, execute, run, perform)
- Word-boundary matching pour préfixes (processOrder flaggé, processor autorisé)
- Whitelist pour globals Node (process, Buffer, globalThis, console, require)
- handle NOT interdit (idiome React)
- Severity Warning
- Tests exhaustifs (30+ cas)

### no_global_types_file
**Status**: OK
- Détecte fichiers types.ts, src/types.ts, src/types/index.ts, shared/types.ts
- Types doivent être colocalisés avec le code qui les utilise
- Autorise domain/feature types (src/users/types.ts)
- Severity Warning
- Tests couvrent: src/types.ts flaggé, domain types autorisé

### no_globals_shadowing
**Status**: OK
- Détecte variables locales qui shadow des globals
- Liste: console, window, document, process, global, globalThis, setTimeout, setInterval
- Ne flagge que variable_declarator (pas les usages)
- Severity Warning
- Tests couvrent: const console flaggé, let window flaggé, usage autorisé

### no_gratuitous_expression
**Status**: OK
- Détecte expressions toujours vraies/fausses: if(true), if(false), x === x, && false, || true
- Détection self-comparison (x === x toujours true sauf NaN)
- Severity Error - probable bug logique
- Tests couvrent: if(true) flaggé, x === x flaggé, x === y autorisé

### no_hardcoded_ip
**Status**: OK
- TextCheck - détecte IPs IPv4 hardcodées dans les strings
- Parser manuel de dotted-quad (octets 0-255)
- Whitelist: 127.0.0.1, 0.0.0.0
- Vérifie présence de quotes sur la ligne (évite commentaires)
- Severity Error
- Tests couvrent: 192.168.1.1 flaggé, localhost autorisé, commentaire sans quotes ignoré

### no_hex_escape
**Status**: OK
- Détecte `\xNN` hex escapes dans les strings
- Suggère `\u00NN` (Unicode escape plus explicite)
- Gère les backslashes échappés (\\x pas flaggé)
- Severity Warning
- Tests couvrent: \x41 flaggé, A autorisé, \\x41 autorisé

### no_identical_conditions
**Status**: OK
- Détecte conditions dupliquées dans chaînes if/else-if
- Parcourt toute la chaîne depuis le if racine
- Compare les textes de condition
- Severity Error - seconde branche jamais exécutée
- Tests couvrent: duplicate flaggé, différentes autorisées, multiples duplicates

### no_identical_expressions
**Status**: OK
- Détecte `x OP x` où OP est ===, !==, &&, ||, -, /
- Résultat prévisible ou toujours 0/true/false
- Évite FPs sur tokens courts pour - et /
- Severity Error
- Tests couvrent: a === a flaggé, valid && valid flaggé, count - count flaggé

### no_identical_title
**Status**: OK
- Détecte describe/test/it avec même titre dans même scope lexical
- Scope = callback body d'un describe ou racine program
- Gère .only/.skip comme même construct
- Ignore titres dynamiques (template avec substitution)
- Récursion dans les describe imbriqués
- Severity Warning
- Tests exhaustifs (12+ cas)

### no_ignored_exceptions
**Status**: OK
- Détecte catch blocks vides ou avec seulement des commentaires
- Exceptions silencieuses cachent les bugs
- Severity Error
- Tests couvrent: catch vide flaggé, avec comments flaggé, avec handler autorisé

### no_ignored_return
**Status**: OK
- Détecte appels standalone à méthodes pures: map, filter, slice, concat, trim, replace, etc.
- Le retour est ignoré, l'appel n'a aucun effet
- Vérifie expression_statement > call_expression > member_expression
- Severity Warning
- Tests couvrent: arr.map() flaggé, items.filter() flaggé, const x = arr.map() autorisé

### no_immediate_mutation
**Status**: OK
- Détecte mutation immédiate après déclaration: `const arr = []; arr.push(1);`
- Couvre arrays (sort, push, reverse), objects (prop assignment), Set (add), Map (set)
- Suggère de chaîner sur l'initialiseur
- Vérifie que le next sibling est bien une mutation du même nom
- Severity Warning
- Tests exhaustifs (10+ cas)

### no_implicit_deps
**Status**: OK
- TextCheck - vérifie que les imports bare sont dans package.json
- Résolution: skip relatifs, skip node: builtins, check tsconfig paths, lookup deps
- Cherche dans dependencies, devDependencies, peerDependencies, optionalDependencies, engines
- Silencieux si pas de package.json trouvé
- Severity Warning

### no_import_dist
**Status**: OK
- Détecte imports vers `/dist/` ou `dist/`
- Couvre import, require, dynamic import
- Les imports dist sont fragiles - import depuis le point d'entrée
- Ne flag pas "distance" (substring check correct)
- Severity Warning
- Tests couvrent: pkg/dist/foo flaggé, ./dist/bar flaggé, pkg autorisé

### no_in_misuse
**Status**: OK
- Détecte `x in array` où array ressemble à un tableau (heuristique par nom)
- `in` vérifie les clés d'objet, pas les valeurs de tableau
- Suggère .includes()
- Skip for...in loops
- Severity Error
- Tests couvrent: in sur myItems flaggé, in sur config autorisé, for...in autorisé

### no_incomplete_assertions
**Status**: OK
- Détecte `expect(x);` sans matcher dans fichiers test
- Détecte aussi `expect(x).not;` sans matcher
- Liste exhaustive de matchers Jest/Vitest
- Ne s'active que sur .test./.spec./__tests__
- Severity Error - test ne teste rien
- Tests couvrent: bare expect flaggé, expect.not flaggé, expect.toBe autorisé

### no_inconsistent_returns
**Status**: OK
- Détecte fonctions mélangeant `return value;` et `return;`
- Parse textuel avec tracking de profondeur de braces
- Skip les fonctions imbriquées
- Severity Warning
- Tests couvrent: mix flaggé, consistent values autorisé, consistent bare autorisé

### no_incorrect_string_concat
**Status**: OK
- Détecte `"..." + numericVar` - concaténation string + nombre
- Heuristique par noms: count, num, total, index, length, size, amount, etc.
- Suggère conversion explicite ou template literals
- Severity Warning
- Tests couvrent: "Total: " + itemCount flaggé, string + userName autorisé

### no_index_file
**Status**: OK
- TextCheck - flag les fichiers `index.{ts,tsx,js,jsx,mjs,cjs}` qui sont des barrel files
- Détecte les re-exports: `export * from`, `export { } from`
- Ne flag pas si le fichier contient de vraies implémentations
- Message: cause du bloat bundler et risque de circular imports
- Severity Warning
- Tests couvrent: index.ts avec `export *` flaggé, index.ts avec fonction autorisé

### no_indexof_equality
**Status**: OK
- Détecte comparaisons indexOf avec -1 ou 0 - suggère `includes()` ou `startsWith()`
- `indexOf() !== -1` → `includes()`
- `indexOf() === 0` → `startsWith()`
- `indexOf() > -1` → `includes()`
- Severity Warning
- Tests couvrent: tous les patterns de comparaison

### no_inferred_any
**Status**: OK
- Détecte les patterns qui infèrent `any` en TypeScript
- Flag: `: any =`, `: any;`, `JSON.parse()` sans cast, `.json()` sans cast
- Skip si `as Type` ou `satisfies Type` présent
- Only active sur .ts/.tsx (pas .js)
- Severity Warning
- Tests couvrent: JSON.parse flaggé, JSON.parse as Config autorisé, .json() flaggé

### no_inline_function_event_listener
**Status**: OK
- Détecte `addEventListener('x', () => ...)` avec callback inline
- Arrow functions et function expressions flaggés
- Problème: impossible de removeEventListener sans référence
- Suggère d'extraire vers une fonction nommée
- Severity Warning
- Tests couvrent: arrow flaggé, function expression flaggé, référence identifieur autorisé

### no_inner_html
**Status**: OK
- Détecte `.innerHTML = ...` et `.outerHTML = ...`
- Assignment et augmented assignment (`+=`)
- XSS sink - suggère textContent ou DOMPurify
- Ne flag pas la lecture (const s = el.innerHTML)
- Severity Error
- Tests couvrent: assignment flaggé, += flaggé, textContent autorisé, lecture autorisée

### no_insecure_jwt
**Status**: OK
- Détecte configurations JWT faibles: `algorithm: 'none'`, `algorithms: ['none']`
- Flag aussi HS256 dans contexte JWT (préférer RS256/ES256)
- Pattern textuel dans ast_check
- Severity Error
- Tests couvrent: algorithm none flaggé, HS256 dans jwt.sign flaggé, RS256 autorisé

### no_instanceof_builtins
**Status**: OK
- Détecte `x instanceof Array/Error/Promise/Map/Set/RegExp/etc.`
- instanceof échoue entre realms (iframes, workers)
- Suggère Array.isArray() pour Array
- Liste: Array, ArrayBuffer, Error, *Error, RegExp, Promise, Map, Set, WeakMap, WeakSet
- Severity Warning
- Tests exhaustifs (10+ cas)

### no_interpolation_in_snapshots
**Status**: OK
- Détecte template literals avec interpolation dans toMatchSnapshot/toMatchInlineSnapshot
- Interpolation rend le snapshot instable (tautologie)
- Vérifie `template_substitution` child dans `template_string`
- Severity Warning
- Tests couvrent: interpolation flaggée, plain template autorisé, pas d'args autorisé

### no_invalid_fetch_options
**Status**: OK
- Détecte `fetch()` ou `new Request()` avec `body` sur GET/HEAD
- Parse multi-lignes avec tracking de profondeur de parens
- Ignore body: null, body: undefined, et spread ...options
- Severity Error
- Tests exhaustifs (10 cas): body+GET flaggé, body+POST autorisé, multiline supporté

### no_invalid_remove_event_listener
**Status**: OK
- Détecte `removeEventListener` avec callback inline ou .bind()
- Arrow functions, function expressions, et .bind() créent de nouvelles références
- Parse textuel avec gestion des quotes et parens
- Severity Warning
- Tests couvrent: .bind() flaggé, arrow flaggé, référence fonction autorisée

### no_invariant_returns
**Status**: OK
- Détecte fonctions qui retournent toujours la même valeur littérale
- Parse textuel: track profondeur braces, collecte return statements
- Flag si 2+ returns et tous identiques (true, false, null, number, string)
- Suggère d'utiliser une constante
- Severity Warning
- Tests couvrent: invariant true flaggé, different returns autorisé, single return autorisé

### no_inverted_boolean_check
**Status**: OK
- Détecte `!a === b` patterns (négation avant comparaison)
- Parse byte par byte: skip `!==` operator, cherche `!identifier ===`
- Supporte member access: `!foo.bar === baz`
- Suggère `a !== b` ou `!(a === b)`
- Severity Warning
- Tests couvrent: !a===b flaggé, !a!==b flaggé, a===b autorisé, !(a===b) autorisé

### no_json_parse_cast
**Status**: OK
- Détecte `JSON.parse(x) as T` - cast dangereux
- Utilise AstCheck + walker (pas macro), cherche `as_expression` avec `call_expression` JSON.parse
- Le cast est un mensonge: la forme runtime peut ne pas matcher T
- Suggère Zod schema ou type guard
- Severity Error
- Tests couvrent: JSON.parse as User flaggé, Schema.parse(JSON.parse()) autorisé

### no_keyword_prefix
**Status**: OK
- Détecte identifiants préfixés par `new` ou `class` + majuscule (newUser, classNames)
- Ne flag que les sites de déclaration (pas les usages)
- Liste exhaustive de declaration sites: variable_declarator, function_declaration, etc.
- Skip si minuscule après le mot-clé (newborn, classify)
- Severity Warning
- Tests couvrent: newUser flaggé, classNames flaggé, newborn autorisé

### no_large_snapshots
**Status**: OK
- Détecte inline snapshots trop longs (toMatchInlineSnapshot, toThrowErrorMatchingInlineSnapshot)
- Seuil configurable via `ctx.config.threshold("no-large-snapshots", "max_lines")`
- Compte les lignes du template_string ou string argument
- Severity Warning
- Tests couvrent: 60 lignes flaggé, 2 lignes autorisé, pas d'args ignoré

### no_let
**Status**: OK
- Détecte `let` declarations
- Vérifie le premier child de `lexical_declaration` pour distinguer let/const
- `let` crée un binding mutable - préférer `const`
- Ne flag pas `var` (node type différent: variable_declaration)
- Severity Warning
- Tests couvrent: let x flaggé, const x autorisé, var x ignoré

### no_logger_in_business_logic
**Status**: OK
- Détecte logging (console.*, logger.*) dans les couches métier
- Dirs business: service, domain, core, model, entity
- Skip les commentaires
- Suggère withLogging() wrapper ou domain events
- Severity Warning
- Tests utilisent run_path pour tester les chemins spécifiques

### no_lonely_if
**Status**: OK
- Détecte `else { if (x) {} }` - should be `else if`
- Vérifie: if_statement seul enfant de statement_block dans else_clause
- Différent de no-collapsible-if qui fusionne `if (a) { if (b) {} }`
- Severity Warning
- Tests couvrent: lonely if flaggé, else if autorisé, else avec multiple statements autorisé

### no_loop_counter_reassign
**Status**: OK
- Détecte réassignation du compteur de boucle for dans le body
- Parse textuel: extrait var name de `for (let/var/const IDENT`, track profondeur braces
- Vérifie `varname = ` (pas `varname ==`)
- Severity Error
- Tests couvrent: i = 5 dans boucle flaggé, console.log(i) autorisé

### no_magic_array_flat_depth
**Status**: OK
- Détecte `.flat(N)` où N est un magic number (!= 1)
- Skip: .flat(), .flat(1), .flat(Infinity), .flat(variable)
- Suggère constante nommée ou Infinity
- Severity Warning
- Tests exhaustifs (8 cas): flat(3) flaggé, flat(1) autorisé, flat(Infinity) autorisé

### no_manual_rtl_cleanup
**Status**: OK
- Détecte import de `cleanup` depuis `@testing-library/*` dans fichiers test
- Vitest exécute cleanup automatiquement après chaque test
- Ne s'active que sur .test., .spec., __tests__, _test.
- Recurse dans import_specifiers pour trouver `cleanup`
- Severity Warning
- Tests couvrent: cleanup seul flaggé, cleanup parmi autres flaggé, render seul autorisé

### no_mass_assignment
**Status**: OK
- Détecte `{ ...req.body }` ou `{ ...request.body }` dans appels DB
- Méthodes DB: set, values, insert, update, create
- Mass assignment = vulnérabilité sécurité
- Suggère de picker les champs explicitement
- Severity Error
- Tests couvrent: spread dans .set() flaggé, champs explicites autorisés, spread hors DB autorisé

### no_match_snapshot
**Status**: OK
- Détecte `toMatchSnapshot()` et `toMatchInlineSnapshot()`
- Utilise AstCheck + walker
- Snapshots = maintenance trap, refactors cassent et devs update aveuglément
- Suggère assertions sur champs spécifiques
- Severity Warning
- Tests couvrent: toMatchSnapshot flaggé, toBe autorisé

### no_misleading_array_reverse
**Status**: OK
- Détecte assignment/return de méthodes mutantes: .reverse(), .sort(), .fill(), .splice()
- Ces méthodes retournent la même référence, pas une copie
- Skip `[...arr].reverse()` pattern (copie puis mutation)
- Severity Error
- Tests couvrent: const x = arr.reverse() flaggé, arr.reverse() seul autorisé, spread copy autorisé

### no_misleading_collection_name
**Status**: OK
- Détecte mismatch entre suffixe du nom (List, Set, Map, Array) et type réel
- Utilise AstCheck + walker, analyse variable_declarator
- Shapes: Array ([], new Array), Set (new Set), Map (new Map)
- Skip initialiseurs inconnus (appels de fonction)
- Severity Error
- Tests couvrent: userList = new Set() flaggé, userSet = [] flaggé, userList = [] autorisé

### no_misplaced_loop_counter
**Status**: OK
- Détecte boucles for avec condition et update sur variables différentes
- Extrait var de condition (binary_expression left) et update (update_expression, augmented_assignment)
- Likely copy-paste bug
- Severity Error
- Tests couvrent: i < n; j++ flaggé, i < n; i++ autorisé, ++i autorisé

### no_mock_fetch_directly
**Status**: OK
- Détecte mocking direct de HTTP clients dans fichiers test
- vi.mock('axios'), jest.mock('node-fetch')
- global.fetch = vi.fn(), globalThis.fetch = jest.fn()
- Suggère MSW pour intercepter au niveau réseau
- Ne s'active que sur .test., .spec., __tests__
- Severity Warning
- Tests couvrent: vi.mock('axios') flaggé, global.fetch = vi.fn() flaggé

### no_mocks_import
**Status**: OK
- Détecte imports depuis `__mocks__` directory
- Jest/Vitest auto-resolve les mocks, pas besoin d'import direct
- Vérifie si spec contient `__mocks__`
- Severity Warning
- Tests couvrent: ./__mocks__/foo flaggé, ./foo autorisé

### no_multi_op_oneliner
**Status**: OK
- Détecte lignes avec 4+ opérateurs chaînés (filter().map().reduce() * tax + discount)
- Utilise dense_lines::scan_dense_lines helper
- Exclut les opérateurs dans les commentaires trailing
- Suggère d'extraire des variables intermédiaires nommées
- Severity Warning
- Tests couvrent: chaîne dense flaggée, a + b autorisé, commentaires trailing ignorés

### no_mutable_exports
**Status**: OK
- Détecte `export let` et `export var`
- Vérifie export_statement avec lexical_declaration (let) ou variable_declaration (var)
- Suggère `export const`
- Severity Warning
- Tests couvrent: export let flaggé, export var flaggé, export const autorisé

### no_mutating_assign
**Status**: OK
- Détecte `Object.assign(target, ...)` où target n'est pas `{}`
- Object.assign mute le premier argument in place
- Skip `Object.assign({}, foo, bar)` - pattern non-mutant
- Suggère `{...target, ...source}` ou `Object.assign({}, target, source)`
- Severity Warning
- Tests couvrent: Object.assign(foo, bar) flaggé, Object.assign({}, foo) autorisé

### no_mutating_methods
**Status**: OK
- Détecte appels à méthodes array mutantes: push, pop, shift, unshift, splice, sort, reverse, fill, copyWithin
- Heuristique par nom (pas de résolution de type)
- Suggère alternatives non-mutantes: spread, slice, toSorted, toReversed, toSpliced
- Severity Warning
- Tests couvrent: arr.push() flaggé, arr.toSorted() autorisé, push(arr, 1) ignoré

### no_mutation
**Status**: OK
- Détecte mutations sur bindings `const`
- Couvre: property assignment, compound assignment, mutating methods, ++/--, delete, Object.assign/defineProperty
- Résolution de scope légère: remonte jusqu'à trouver `const <name>`
- Supporte destructuring: `const { a } = ...`
- Severity Warning
- Tests exhaustifs (25+ cas): obj.prop = 1 flaggé, arr.push() flaggé, let autorisé, spread autorisé

### no_named_default
**Status**: OK
- Détecte `import { default as foo }` patterns
- Suggère `import foo from '...'` - syntaxe default idiomatique
- Recurse dans import_specifiers pour trouver `name == "default"`
- Severity Warning
- Tests couvrent: { default as foo } flaggé, import foo autorisé

### no_namespace_import
**Status**: OK
- Détecte `import * as ...` patterns
- Vérifie si le texte de l'import contient `* as `
- Suggère named imports
- Severity Warning
- Tests couvrent: import * as utils flaggé, { foo, bar } autorisé, import utils autorisé

### no_negated_condition
**Status**: OK
- Détecte conditions négatives dans if/else et ternaires
- Patterns: `if (!x)`, `if (a !== b)`, `!x ? a : b`
- Skip if sans else, skip else if chains
- Suggère de swap les branches et enlever la négation
- Severity Warning
- Tests couvrent: !x if/else flaggé, a !== b flaggé, if sans else autorisé, else if autorisé

### no_negation_in_equality_check
**Status**: OK
- Détecte `!x === y` (bug de précédence)
- `!x === y` est parsé comme `(!x) === y`, pas `!(x === y)`
- Skip double négation `!!x === true` (coercition bool intentionnelle)
- Suggère `x !== y` ou `!(x === y)`
- Severity Error
- Tests couvrent: !x === true flaggé, !!x === true autorisé, x === !y autorisé

### no_nested_assignment
**Status**: OK
- Détecte assignments dans conditions: `if (x = 10)`, `while (node = node.next)`
- Recurse pour trouver assignment_expression dans la condition
- Probablement un bug, devrait être `===` ou sortir l'assignment
- Severity Error
- Tests couvrent: x = 10 flaggé, x === 10 autorisé, x <= 10 autorisé

### no_nested_functions
**Status**: OK
- Détecte fonctions imbriquées 3+ niveaux de profondeur
- Compte les ancêtres function_declaration/function_expression
- Suggère d'extraire au module scope
- Severity Warning
- Tests couvrent: outer > middle > tooDeep flaggé, outer > inner (2 niveaux) autorisé

### no_nested_incdec
**Status**: OK
- Détecte `++`/`--` utilisé à l'intérieur d'expressions
- Skip standalone statement, skip for loop update clause
- Suggère de séparer en statement propre pour clarté
- Severity Warning
- Tests couvrent: arr[i++] flaggé, i++; autorisé, for (;; i++) autorisé

### no_nested_switch
**Status**: OK
- Détecte switch imbriqué dans un autre switch
- Walk ancestors pour trouver parent switch_statement
- Suggère d'extraire le switch interne dans une fonction
- Severity Error
- Tests couvrent: switch dans switch flaggé (1 diag), 3 niveaux (2 diags), séquentiels autorisés

### no_nested_template_literal
**Status**: OK
- Détecte template literal contenant un autre template literal dans interpolation
- ``\`a ${\`b\`}\`` est hard to read
- Vérifie si descendant est template_string (structure AST)
- Ne flag PAS multiple interpolations dans le même template
- Severity Error
- Tests couvrent: template nested flaggé, `${foo}/api/${id}` autorisé, String(id) autorisé

### no_nested_ternary
**Status**: OK
- Détecte ternaires imbriqués (ternary dont parent est ternary)
- Utilise AstCheck + walker
- Suggère if/else ou variables nommées par branche
- Severity Error
- Tests couvrent: a ? b ? 1 : 2 : 3 (1 diag), 3 niveaux (2 diags), simple ternary autorisé

### no_new_regex_with_variable
**Status**: OK
- Détecte `new RegExp(variable)` - risque ReDoS
- Pattern crafté peut freeze l'event loop via backtracking exponentiel
- Skip string literals et template literals (safe)
- Suggère regex littéral ou bibliothèque safe-regex
- Severity Error
- Tests couvrent: new RegExp(userInput) flaggé, new RegExp('foo') autorisé, /foo/ autorisé

### no_null
**Status**: OK
- Détecte utilisation du littéral `null`
- Vérifie node.kind() == "null"
- Skip si parent est commentaire
- Suggère `undefined` à la place
- Severity Warning
- Tests couvrent: const x = null flaggé, x === null flaggé, undefined autorisé

### no_object_as_default_parameter
**Status**: OK
- Détecte `function f(opts = { key: val })` - objet literal comme default
- Gère assignment_pattern (JS) et required/optional_parameter (TS)
- Skip objets vides `= {}` (OK)
- Suggère destructuring avec defaults individuels
- Severity Warning
- Tests couvrent: opts = { timeout: 1000 } flaggé, opts = {} autorisé, { timeout = 1000 } = {} autorisé

### no_one_iteration_loop
**Status**: OK
- Détecte boucles qui terminent toujours à la première itération
- Vérifie: dernier statement est return/break/throw inconditionnels
- Skip si continue présent dans statements précédents
- La boucle est redondante
- Severity Warning
- Tests couvrent: for avec return final flaggé, while avec break flaggé, if (cond) break autorisé

### no_open_redirect
**Status**: OK
- Détecte `res.redirect(userInput)` avec données utilisateur
- User data: req.query, req.params, req.body, searchParams.get
- Vulnérabilité open redirect
- Suggère validation contre allowlist
- Severity Error
- Tests couvrent: req.query.returnUrl flaggé, '/dashboard' autorisé, safeUrl autorisé

### no_page_click_deprecated
**Status**: OK
- Détecte Playwright deprecated: page.click(), page.fill(), page.type(), page.check(), page.uncheck()
- Ne s'active que sur fichiers test (.test., .spec., __tests__, .e2e.)
- Suggère `page.locator(selector).method()` à la place
- Severity Warning
- Tests couvrent: page.click() flaggé, page.locator().click() autorisé, non-test ignoré

### no_path_traversal
**Status**: OK
- Détecte fs.readFile/writeFile/unlink/etc. avec chemin user-controlled
- User data: req.params, req.query, req.body, searchParams.get
- Skip si sanitized: path.basename(), path.resolve(), normalize()
- Vulnérabilité path traversal
- Severity Error
- Tests couvrent: req.params.filename flaggé, path.basename() autorisé, littéral autorisé

### no_post_message_star
**Status**: OK
- Détecte `postMessage(data, "*")` - targetOrigin wildcard
- Vérifie 2e argument est string "*"
- Vulnérabilité: message peut être intercepté par n'importe quel origin
- Suggère origin explicite
- Severity Error
- Tests couvrent: "*" flaggé, "https://example.com" autorisé, variable autorisée

### no_primitive_wrappers
**Status**: OK
- Détecte `new String()`, `new Number()`, `new Boolean()`
- Ces wrappers créent des objets, pas des primitives
- Suggère appel factory sans `new`: `String()`, `Number()`, `Boolean()`
- Severity Error
- Tests couvrent: new String() flaggé, String() autorisé, new Map() autorisé

### no_process_exit
**Status**: OK
- Détecte `process.exit()`
- Skip fichiers avec shebang (scripts CLI légitimes)
- Termine abruptement - suggère de throw une erreur
- Severity Warning
- Tests couvrent: process.exit(1) flaggé, shebang file autorisé

### no_promise_reject
**Status**: OK
- Détecte `Promise.reject()`
- Suggère de retourner des valeurs d'erreur ou throw typed errors
- Severity Warning
- Tests couvrent: Promise.reject() flaggé, Promise.resolve() autorisé

### no_prototype_pollution
**Status**: OK
- Détecte deep-merge avec user data: _.merge, lodash.merge, deepMerge, Object.assign
- User data: req.body, request.body, JSON.parse
- Risque de prototype pollution
- Suggère de sanitize input avant merge
- Severity Error
- Tests couvrent: _.merge(config, req.body) flaggé, _.merge(config, defaults) autorisé

### no_pseudo_random
**Status**: OK
- Détecte `Math.random()`
- Non cryptographiquement sécurisé
- Suggère crypto.randomUUID() ou crypto.getRandomValues()
- Severity Warning
- Tests couvrent: Math.random() flaggé, Math.floor(Math.random()*1000) flaggé, crypto.randomUUID() autorisé

### no_raw_db_entity_in_handler
**Status**: OK
- Détecte appels DB directs dans route handlers
- DB patterns: prisma, db, knex + findMany, findFirst, query
- Route methods: get, post, put, delete, patch
- Suggère mapper vers DTO avant de retourner
- Severity Warning
- Tests couvrent: prisma.user.findMany() dans app.get() flaggé, handler sans DB autorisé

### no_redundant_assignment
**Status**: OK
- Détecte variable assignée puis immédiatement réécrite ligne suivante
- Parse textuel: extrait assignment target de chaque ligne
- Skip const (reassign serait erreur syntaxe)
- Skip comments, control flow, return
- Severity Error
- Tests couvrent: let x = 1; x = 2 flaggé, console.log(x) entre autorisé

### no_redundant_await
**Status**: OK
- Détecte `return await x;` hors d'un bloc try
- Dans try, return await est utile (rejections catchables)
- Hors try, c'est une microtask inutile
- Utilise AstCheck + walker, vérifie ancestors jusqu'à function boundary
- Severity Warning
- Tests couvrent: return await g() flaggé, try { return await } autorisé, catch { return await } flaggé

### no_redundant_boolean
**Status**: MINOR
- Détecte patterns boolean redondants: `? true : false`, `=== true`, `if () return true; return false;`
- Parse textuel ligne par ligne
- Skip commentaires
- Suggère d'utiliser la condition directement
- Severity Error
- **Note**: Pattern hybride text/AST dans ast_check! - risque de FP sur patterns dans strings

### no_redundant_clsx
**Status**: OK
- Détecte `clsx("foo")` ou `cn("foo")` avec un seul string argument statique
- Ajoute un wrapper runtime sans logique conditionnelle
- Skip: variables, template literals, multiple args, objects
- Suggère d'utiliser le string directement
- Severity Warning
- Tests exhaustifs (10+ cas)

### no_redundant_jump
**Status**: OK
- Détecte `return;` et `continue;` redondants (exécution fall-through déjà)
- Walk up ancestors through tail positions vers callable/loop boundary
- Skip return avec valeur, continue avec label
- Severity Warning
- Tests couvrent: return; en fin de fonction flaggé, continue; en fin de boucle flaggé, early return autorisé

### no_redundant_optional
**Status**: OK
- Détecte `?:` avec `| undefined` - redondant
- `?:` implique déjà `| undefined`
- Parse textuel simple
- Severity Warning
- Tests couvrent: name?: string | undefined flaggé, name?: string autorisé, name: string | undefined autorisé

### no_return_type_any
**Status**: OK
- Détecte fonctions avec `): any` return type explicite
- Patterns: `): any {`, `): any =>`, `): Promise<any>`
- Parse textuel
- Suggère type spécifique ou `unknown`
- Severity Error
- Tests couvrent: (): any flaggé, (): Promise<any> flaggé, (): string autorisé

### no_same_argument_assert
**Status**: OK
- Détecte `expect(x).toBe(x)` - assertion qui compare une valeur à elle-même
- Ne s'active que sur fichiers test
- Matchers: toBe, toEqual
- Toujours vrai, ne teste rien
- Severity Error
- Tests couvrent: expect(x).toBe(x) flaggé, expect(actual).toBe(expected) autorisé

### no_section_divider_comments
**Status**: OK
- TextCheck - détecte commentaires avec caractères répétés: `// =====`, `// *****`, `// -----`
- Seuil configurable (min_run, default 5)
- Dividers signalent que le fichier fait trop de choses
- Suggère de split le fichier par responsabilité
- Severity Error
- Tests couvrent: // ====== flaggé, // -- note autorisé, string dans code ignoré

### no_self_import
**Status**: OK
- TextCheck - détecte module qui s'importe lui-même
- Patterns: `import from '.'`, `import from './index'`, `import from './foo'` dans foo.ts
- Parse textuel, extrait source après `from`
- Severity Error
- Tests couvrent: import '.' dans index.ts flaggé, import './utils' dans utils.ts flaggé

### no_set_x_to_y
**Status**: OK
- Détecte noms de fonction pattern `setXToY` (setStatusToClosed, setRoleToAdmin)
- Encode l'implémentation, pas l'intention
- Utilise AstCheck + walker, check function_declaration, method_definition, arrow const
- Suggère renommer: setStatusToClosed → closeAccount
- Severity Error
- Tests couvrent: setStatusToClosed flaggé, setUser autorisé, setupDatabase autorisé

### no_shell_exec
**Status**: OK
- Détecte exec/spawn avec interpolation ou { shell: true }
- Functions: exec, execSync, spawn, spawnSync
- Template literal avec substitution = injection
- Suggère execFile() avec args array
- Severity Error
- Tests couvrent: exec(`git ${cmd}`) flaggé, { shell: true } flaggé, exec('git status') autorisé

### no_side_effects_in_initialization
**Status**: OK
- Détecte appels top-level (expression_statement directement sous program)
- Flagge call_expression et new_expression au niveau module
- Bloque le tree-shaking — exécution à l'import
- Respecte annotation `/*#__PURE__*/` et `/*@__PURE__*/`
- Severity Warning
- Tests couvrent: bare call flaggé, new flaggé, IIFE flaggé, call dans fonction autorisé, /*#__PURE__*/ autorisé

### no_single_promise_in_promise_methods
**Status**: OK
- Détecte `Promise.all([single])`, `Promise.any([single])`, `Promise.race([single])`
- Wrapper inutile — utiliser la valeur directement
- Skip spread elements `[...promises]` (valide)
- N'inclut PAS `Promise.allSettled` (sémantique différente)
- Severity Warning
- Tests couvrent: all/any/race avec 1 élément flaggé, multiples éléments autorisés, spread autorisé

### no_small_switch
**Status**: OK
- Détecte switch avec moins de 3 cases
- Parse textuel comptant `case ` dans le body du switch
- Suggère if/else pour 1-2 cases
- Severity Warning
- Tests couvrent: 1 case flaggé, 2 cases flaggé, 3+ cases autorisé

### no_sort_without_comparator
**Status**: OK
- Détecte `.sort()` sans argument comparateur
- Tri lexicographique par défaut = bug fréquent sur nombres
- Parse textuel cherchant `.sort(` suivi de `)`
- Gère whitespace `.sort(  )`
- Severity Error
- Tests couvrent: .sort() flaggé, .sort(  ) flaggé, .sort((a,b) => a-b) autorisé

### no_ssrf_fetch
**Status**: OK
- Détecte fetch/axios/got/http avec URL construite depuis user input
- Liste fonctions: fetch, got, request, ky, axios.*, http.*, https.*
- Données user: req.query, req.params, req.body, searchParams.get
- Suggère validation contre allowlist
- Severity Error - faille SSRF
- Tests couvrent: fetch(req.query.x) flaggé, axios.get(req.body.url) flaggé, fetch('literal') autorisé

### no_static_only_class
**Status**: OK
- Détecte classes où tous les membres sont static
- Préfère object literal ou fonctions pures (tree-shakeable)
- Skip classes avec héritage (extends)
- Gère class declarations et class expressions
- Severity Warning
- Tests couvrent: static methods only flaggé, static fields only flaggé, instance method autorisé, extends autorisé

### no_submit_handler_without_prevent_default
**Status**: OK
- Détecte `onSubmit={(e) => ...}` sans appel à `preventDefault()`
- Parcours récursif du body pour trouver `preventDefault()`
- Ignore handlers référencés (identifiers) — scope tracking hors portée
- Severity Warning
- Tests couvrent: arrow sans preventDefault flaggé, function expr flaggé, avec preventDefault autorisé, handler référencé ignoré

### no_sync_scripts
**Status**: OK
- Détecte `<script src="...">` sans async/defer
- Scripts sync bloquent le parsing HTML
- Ignore scripts inline (pas de src)
- JSX only (jsx_opening_element, jsx_self_closing_element)
- Severity Warning
- Tests couvrent: script sync flaggé, async autorisé, defer autorisé, inline autorisé

### no_test_imports_in_prod
**Status**: OK
- Détecte imports de modules test depuis fichiers production
- Markers test: `.test.`, `.spec.`, `__tests__`, `__mocks__`
- Skip si le fichier courant est lui-même un test
- Évite de shipper fixtures dans le bundle
- Severity Warning
- Tests utilisent run_ts_with_path pour simuler différents chemins

### no_test_logic
**Status**: OK
- Détecte if/for/while/switch dans le body d'un test
- Tests devraient avoir un chemin linéaire d'assertions
- Ignore control-flow dans beforeEach/afterEach/beforeAll/afterAll
- Active seulement sur fichiers test (`.test.`, `.spec.`, `__tests__`)
- Severity Warning
- Tests couvrent: if dans test flaggé, for dans test flaggé, fichier non-test ignoré

### no_test_prefixes
**Status**: OK
- Détecte préfixes Jasmine: ftest, fdescribe, fit, xtest, xdescribe, xit
- f = focus, x = skip — changent le comportement CI silencieusement
- Préfère .only/.skip explicites
- Severity Warning
- Tests couvrent: chaque préfixe flaggé, test/describe/it normaux autorisés, test.only autorisé

### no_test_return_statement
**Status**: OK
- Détecte `return` dans le callback direct de test()/it()
- Return ignoré par le runner — préférer expect assertions
- Parcours ascendant pour trouver la première fonction englobante
- Autorise return dans fonctions helper imbriquées
- Severity Warning
- Tests couvrent: return dans arrow flaggé, return dans nested helper autorisé

### no_thenable
**Status**: OK
- Détecte objets/classes avec propriété `then`
- Thenables sont unwrap par await/Promise.resolve — bugs async subtils
- Couvre: pair dans object, method_definition, public_field_definition, exports
- Severity Warning
- Tests couvrent: { then() {} } flaggé, class then method/field flaggé, export then flaggé

### no_this_assignment
**Status**: OK
- Détecte `const self = this`, `let that = this`, `_this = this`
- Pattern pré-arrow-function obsolète
- Gère variable_declarator et assignment_expression
- Severity Warning
- Tests couvrent: const/let/var flaggé, assignment flaggé, this.foo autorisé

### no_this_mutation
**Status**: OK
- Détecte `this.prop = value` hors constructor
- Parcours ascendant pour vérifier si dans constructor
- Autorise: constructor, field initializers (class body direct)
- Severity Warning
- Tests couvrent: mutation dans method flaggé, constructor autorisé, field initializer autorisé, setter flaggé

### no_throw
**Status**: OK
- Détecte tout throw_statement
- Philosophie: Result<T, E> au lieu d'exceptions (erreurs comme valeurs)
- Simple match sur node.kind()
- Severity Error
- Tests couvrent: throw flaggé, multiples throws flaggés, code sans throw autorisé

### no_timing_attack
**Status**: OK
- Détecte comparaison directe (==/===/!=/!==) de valeurs sensibles
- Mots sensibles: password, token, signature, hash, apiKey, secret
- Ignore literals et call expressions
- Suggère crypto.timingSafeEqual
- Severity Error - faille de sécurité
- Tests couvrent: password===x flaggé, user.token flaggé, req.body.password flaggé, non-sensible autorisé

### no_try_promise
**Status**: OK
- Détecte promise sans await dans try block
- Patterns: .then(, fetch(, axios.*
- Rejet ne sera pas catchée sans await
- Severity Error
- Tests couvrent: fetch sans await flaggé, .then sans await flaggé, await fetch autorisé, hors try autorisé

### no_try_statements
**Status**: OK
- Détecte tout try_statement
- Philosophie: préférer types Result explicites
- Severity Warning
- Tests couvrent: try/catch flaggé, try/finally flaggé, code normal autorisé

### no_type_assertion
**Status**: OK
- Détecte `as T` assertions (as_expression)
- Bypass le type checker — préférer satisfies, type guards, generics
- Exception: `as const` autorisé (refinement, pas cast)
- Severity Error
- Tests couvrent: as string flaggé, as any flaggé, double assertion 2x flaggé, as const autorisé, satisfies autorisé

### no_type_encoded_names
**Status**: OK
- Détecte notation hongroise: strName, arrItems, boolReady, objUser, dblValue
- Vérifie que le préfixe est suivi d'une majuscule (camelCase boundary)
- N'inclut PAS: numItems (number of), intCount (descriptif), fnCallback
- Seulement sur sites de déclaration (variable_declarator, parameter, function_declaration)
- Severity Warning
- Tests couvrent: strName flaggé, arrItems flaggé, userName autorisé, numItems autorisé

### no_typeof_undefined
**Status**: ISSUE
- Détecte `typeof x === 'undefined'`
- Suggère `x === undefined` direct
- **PROBLÈME**: Le conseil peut causer ReferenceError si `x` n'est pas déclaré. `typeof` est le seul moyen safe de vérifier une variable potentiellement non déclarée
- Severity Warning
- Tests couvrent: typeof===undefined flaggé, x===undefined autorisé, typeof===string autorisé

### no_unassigned_import
**Status**: OK
- Détecte imports side-effect `import 'polyfill'`
- Exception: imports CSS/styles (.css, .scss, .sass, .less, .styl, .pcss)
- Vérifie absence de import_clause
- Severity Warning
- Tests couvrent: import 'x' flaggé, import './x.css' autorisé, import { x } from 'y' autorisé

### no_unchecked_json_parse
**Status**: OK
- Détecte `JSON.parse()` sans validation
- JSON.parse retourne `any` — poison pour le type system
- Autorise si wrappé dans .parse() ou .safeParse() (Zod)
- Utilise walk_tree pour traversée complète
- Severity Warning
- Tests couvrent: JSON.parse nu flaggé, schema.parse(JSON.parse(...)) autorisé

### no_undefined_argument
**Status**: OK
- Détecte `undefined` passé comme argument: foo(undefined)
- Suggère d'omettre l'argument
- Check sur node kind "undefined" dans "arguments"
- Severity Warning
- Tests couvrent: foo(undefined) flaggé, foo(x, undefined, y) flaggé, undefinedValue autorisé

### no_undefined_assignment
**Status**: OK
- Détecte `let x = undefined` ou `x = undefined`
- Suggère `let x;` ou `delete obj.prop`
- Gère variable_declarator et assignment_expression
- Severity Warning
- Tests couvrent: let x = undefined flaggé, x = undefined flaggé, x === undefined autorisé

### no_unenclosed_multiline_block
**Status**: OK
- Détecte if/for/while sans accolades avec body sur ligne suivante
- Pattern dangereux: ajout de ligne = bug silencieux
- Autorise one-liner `if (x) doThing();`
- Severity Error
- Tests couvrent: if\n body flaggé, for\n body flaggé, while\n body flaggé, { body } autorisé, one-liner autorisé

### no_uniq_key
**Status**: OK
- Détecte clés JSX non-stables: Math.random(), Date.now(), uuid(), nanoid()
- Nouvelles clés à chaque render = reconciliation cassée
- Check sur jsx_attribute avec name="key"
- Severity Error
- Tests couvrent: Math.random flaggé, Date.now flaggé, uuid() flaggé, item.id autorisé, index autorisé

### no_unnecessary_array_flat_depth
**Status**: OK
- Détecte `.flat(1)` — 1 est la valeur par défaut
- Filtre les arguments pour trouver exactement 1 arg numérique "1"
- Severity Warning
- Tests couvrent: flat(1) flaggé, flat() autorisé, flat(2) autorisé, flat(Infinity) autorisé

### no_unnecessary_array_splice_count
**Status**: OK
- Détecte `.splice(x, arr.length)` ou `.splice(x, Infinity)`
- Count inutile car splice(start) enlève tout depuis start
- Gère aussi toSpliced
- Autorise si 3+ args (avec remplacement)
- Severity Warning
- Tests couvrent: splice(2, arr.length) flaggé, splice(0, Infinity) flaggé, splice(2) autorisé

### no_unnecessary_await
**Status**: OK
- Détecte `await` sur valeurs non-promise évidentes
- Types: number, string, array, arrow_function, function, class, regex, true/false/null/undefined
- Unwrap parenthesized_expression et sequence_expression
- Severity Warning
- Tests couvrent: await 42 flaggé, await 'hello' flaggé, await [1,2] flaggé, await fetch() autorisé

### no_unnecessary_slice_end
**Status**: OK
- Détecte `.slice(x, arr.length)` ou `.slice(x, Infinity)`
- End inutile car slice(start) va jusqu'à la fin
- Même logique que no_unnecessary_array_splice_count
- Severity Warning
- Tests couvrent: slice(2, arr.length) flaggé, slice(0, Infinity) flaggé, slice(2) autorisé

### no_unreadable_array_destructuring
**Status**: OK
- Détecte destructuring avec trous consécutifs `[,, third,,,, seventh]`
- Difficile à compter visuellement — préférer accès par index
- Détection via ",," dans le texte + vérification profondeur
- Severity Warning
- Tests couvrent: [,, third] flaggé, [first,,, fourth] flaggé, [a, , b] autorisé (trou simple)

### no_unreadable_iife
**Status**: OK
- Détecte IIFE avec body parenthésé `(() => (bar))()`
- Confusion entre parens de groupage et invocation
- Unwrap parenthesized_expression pour trouver l'arrow function
- Ignore block body `{ return x; }`
- Severity Warning
- Tests couvrent: (() => (bar))() flaggé, (() => bar)() autorisé, (() => { return bar; })() autorisé

### no_unsafe_alloc
**Status**: OK
- Détecte `Buffer.allocUnsafe()`, `Buffer.allocUnsafeSlow()`, `new Buffer(size)`
- Mémoire non-initialisée = fuite de données sensibles potentielle
- Flag new Buffer seulement si arg est number/identifier/binary_expression
- Severity Error - faille de sécurité
- Tests couvrent: allocUnsafe flaggé, new Buffer(10) flaggé, Buffer.alloc autorisé, Buffer.from autorisé

### no_unsafe_shell_exec
**Status**: OK
- Détecte exec/execSync/spawn/spawnSync avec argument dynamique
- Template strings avec ${} = injection potentielle
- Autorise string literals et template strings sans interpolation
- Suggère execFile/spawn avec argv array
- Severity Error - injection de commande
- Tests couvrent: exec(cmd) flaggé, exec(`ls ${dir}`) flaggé, exec("ls") autorisé

### no_unsanitized_method
**Status**: OK
- Détecte méthodes DOM XSS avec HTML dynamique
- insertAdjacentHTML (arg 1), document.write/writeln (arg 0), setHTMLUnsafe, createContextualFragment
- Safe: string literal ou template sans interpolation
- Severity Error - XSS
- Tests couvrent: insertAdjacentHTML(pos, var) flaggé, document.write avec concat flaggé, literals autorisés

### no_unsanitized_property
**Status**: OK
- Détecte `el.innerHTML = dynamicValue`, `outerHTML`, `srcdoc`
- Seulement assignment_expression (pas +=)
- Safe: string literal ou template sans interpolation
- Severity Error - XSS
- Tests couvrent: innerHTML = var flaggé, template ${} flaggé, literal autorisé, += ignoré, textContent autorisé

### no_unthrown_error
**Status**: OK
- Détecte `new Error(...)` sans throw/return/assignment
- Parse textuel ligne par ligne
- Skip si throw, return, const/let/var =, export, yield
- Severity Error
- Tests couvrent: new Error(...); nu flaggé, throw new Error autorisé, const err = new Error autorisé

### no_unused_collection
**Status**: OK
- Détecte collection écrite mais jamais lue
- Constructors: [], new Map/Set/Array/WeakMap/WeakSet
- Write: push, add, set, unshift, splice
- Read: forEach, map, filter, get, has, length, [...spread], return, passage en arg
- Severity Warning
- Tests couvrent: push sans read flaggé, push + forEach autorisé, push + return autorisé

### no_unvalidated_url_redirect
**Status**: OK
- Détecte redirect client vers URL user-controlled
- Cibles: location.href =, location =, location.replace(), location.assign()
- User data: searchParams.get, req.query/params/body, params., query.
- Severity Error - open redirect
- Tests couvrent: location.href = searchParams.get flaggé, location.replace(query.x) flaggé, literal autorisé

### no_unverified_certificate
**Status**: OK
- Détecte désactivation SSL cert verification
- Patterns: rejectUnauthorized: false, NODE_TLS_REJECT_UNAUTHORIZED, verify: false
- Parse textuel case-insensitive
- Severity Error - permet attaques MITM
- Tests couvrent: rejectUnauthorized: false flaggé, NODE_TLS_... flaggé, verify: false flaggé, true autorisé

### no_valueof_field
**Status**: OK
- Détecte override de `valueOf` sur classes/objects/interfaces
- valueOf change coercion vers primitive — bugs silencieux avec opérateurs
- Gère: method_definition, method_signature, property_signature, pair, public_field_definition
- Severity Warning
- Tests couvrent: class valueOf() flaggé, { valueOf: fn } flaggé, interface valueOf flaggé, data field autorisé

### no_verb_in_rest_url
**Status**: OK
- Détecte verbes dans URLs REST: /api/createOrder, /api/deleteUser
- RPC déguisé en REST — préférer HTTP semantics
- Parse strings contenant /api/ + verb prefix
- Severity Warning
- Tests couvrent: /api/createOrder flaggé, /api/orders autorisé, string non-URL autorisé

### no_wait_for_timeout
**Status**: OK
- Détecte `waitForTimeout()` dans fichiers test (Playwright)
- Sleep fixe = test flaky
- Suggère web-first assertions ou waitForResponse
- Active seulement sur .test., .spec., __tests__, .e2e.
- Severity Error
- Tests couvrent: waitForTimeout dans test flaggé, waitForResponse autorisé, non-test ignoré

### no_weak_cipher
**Status**: OK
- Détecte createCipheriv avec cipher faible: bf, blowfish, des, rc2, rc4
- Suggère aes-256-gcm ou ChaCha20-Poly1305
- Check premier arg string literal, pas constant propagation
- Severity Error - crypto faible
- Tests couvrent: des-ecb flaggé, rc4 flaggé, blowfish flaggé, aes-256-gcm autorisé

### no_weak_hashing
**Status**: OK
- Détecte createHash('md5'), createHash('sha1'), MD5(), SHA1()
- MD5/SHA1 sont cryptographiquement cassés
- Suggère SHA-256 ou plus fort
- Severity Error - crypto faible
- Tests couvrent: md5/sha1 flaggé, MD5()/SHA1() flaggé, sha256 autorisé

### no_weak_keys
**Status**: OK
- Détecte clés RSA faibles (< 2048 bits) et courbes EC faibles (< 256 bits)
- RSA: modulusLength 256/384/512/768/1024
- EC: P-128, P-192, secp192r1
- Severity Error - crypto faible
- Tests couvrent: modulusLength: 1024 flaggé, namedCurve: P-192 flaggé, 2048/P-256 autorisés

### no_weak_ssl
**Status**: OK
- Détecte protocoles SSL/TLS faibles: SSLv2, SSLv3, TLSv1.0, TLSv1.1, TLSv1
- Vérifie strings avec eq_ignore_ascii_case
- Gère edge case TLSv1 vs TLSv1.2/1.3
- Severity Error - protocoles obsolètes vulnérables
- Tests couvrent: SSLv2/SSLv3/TLSv1.0/TLSv1.1 flaggés, TLSv1.2/TLSv1.3 autorisés

### no_while_loop
**Status**: OK
- Détecte while_statement et do_statement
- Philosophie FP: préférer récursion ou higher-order functions
- Severity Warning
- Tests couvrent: while flaggé, do-while flaggé, for-of autorisé, map autorisé

### no_xml_external_entity
**Status**: OK
- Détecte parsers XML sans protection XXE: DOMParser, XMLParser, xml2js
- Vérifie présence de noent: false ou externalEntities: false
- Severity Error - XXE = faille grave
- Tests couvrent: new DOMParser() flaggé, require('xml2js') flaggé, { noent: false } autorisé

### no_zero_fractions
**Status**: OK
- Détecte `1.0`, `2.00`, `3.` — fractions inutiles ou dot dangling
- Noise visuel, préférer `1` au lieu de `1.0`
- Gère underscores numériques `1.0_0`
- Severity Warning
- Tests couvrent: 1.0 flaggé, 1.00 flaggé, 1.5 autorisé, 1 autorisé

### prefer_add_event_listener
**Status**: OK
- Détecte `element.onclick = handler` style assignments
- Liste exhaustive d'events DOM (onclick, onkeydown, onsubmit, etc.)
- Suggère addEventListener pour multiple handlers
- Severity Warning
- Tests OK

### prefer_array_find
**Status**: OK
- Détecte `.filter(...)[0]`, `.filter(...).at(0)`, `.filter(...).shift()`
- find() short-circuits au premier match — plus performant
- Severity Warning
- Tests OK

### prefer_array_flat
**Status**: OK
- Détecte patterns legacy de flatten: `[].concat(...arr)`, `.reduce((a,b) => a.concat(b), [])`
- Préfère .flat() (ES2019)
- Severity Warning
- Tests OK

---
### a11y_html_has_lang
**Status**: OK
- The `<html>` element must have a `lang` attribute.
- Severity Error | TS/Text | 5 tests

### a11y_iframe_has_title
**Status**: OK
- `<iframe>` elements must have a `title` attribute.
- Severity Error | TS/Text | 5 tests

### a11y_img_redundant_alt
**Status**: OK
- `alt` text should not contain redundant words like \
- Severity Warning | TS/Text | 6 tests

### a11y_interactive_supports_focus
**Status**: OK
- Elements with interactive handlers and an interactive role must be focusable.
- Severity Warning | TS/Text | 5 tests

### a11y_label_has_associated_control
**Status**: OK
- `<label>` must have an associated control via `htmlFor` or by wrapping an input.
- Severity Warning | TS/Text | 5 tests

### a11y_media_has_caption
**Status**: OK
- Flag `<video>` and `<audio>` elements without `<track kind=\
- Severity Warning | TS/Text | 5 tests

### a11y_mouse_events_have_key_events
**Status**: OK
- Flag `onMouseOver` without `onFocus` and `onMouseOut` without `onBlur`.
- Severity Warning | TS/Text | 6 tests

### a11y_no_access_key
**Status**: OK
- Avoid using `accessKey` — it conflicts with screen reader keyboard shortcuts.
- Severity Warning | TS/Text | 5 tests

### a11y_no_aria_hidden_on_focusable
**Status**: OK
- Flag `aria-hidden=\
- Severity Error | TS/Text | 7 tests

### a11y_no_distracting_elements
**Status**: OK
- Flag `<marquee>` and `<blink>` elements which are distracting and deprecated.
- Severity Error | TS/Text | 6 tests

### a11y_no_interactive_element_to_noninteractive_role
**Status**: OK
- Interactive elements must not be assigned non-interactive ARIA roles.
- Severity Warning | TS/Text | 6 tests

### a11y_no_noninteractive_element_interactions
**Status**: OK
- Flag non-interactive elements with event handlers but no `role` attribute.
- Severity Warning | TS/Text | 6 tests

### a11y_no_noninteractive_element_to_interactive_role
**Status**: OK
- Non-interactive elements must not be assigned interactive ARIA roles.
- Severity Warning | TS/Text | 6 tests

### a11y_no_noninteractive_tabindex
**Status**: OK
- Flag non-interactive elements with `tabIndex` (other than -1).
- Severity Warning | TS/Text | 6 tests

### a11y_no_redundant_roles
**Status**: OK
- Flag elements with explicit roles matching their implicit ARIA role.
- Severity Warning | TS/Text | 6 tests

### a11y_no_static_element_interactions
**Status**: OK
- Flag `<div>` and `<span>` with `onClick` but no `role` attribute.
- Severity Warning | TS/Text | 6 tests

### a11y_prefer_tag_over_role
**Status**: OK
- Prefer semantic HTML elements over `role` attributes on generic elements.
- Severity Warning | TS/Text | 6 tests

### a11y_role_has_required_aria_props
**Status**: OK
- Elements with ARIA roles must have all required ARIA properties.
- Severity Error | TS/Text | 7 tests

### a11y_scope
**Status**: OK
- The `scope` attribute should only be used on `<th>` elements.
- Severity Error | TS/Text | 5 tests

### better_auth_no_disable_csrf
**Status**: OK
- `disableCSRFCheck: true` removes CSRF protection from Better Auth.
- Severity Error | TS | 3 tests

### better_auth_no_disable_origin_check
**Status**: OK
- `disableOriginCheck: true` removes origin validation from Better Auth.
- Severity Error | TS | 3 tests

### better_auth_plugin_import_path
**Status**: OK
- Importing from `better-auth/plugins` barrel prevents tree-shaking.
- Severity Warning | TS | 4 tests

### better_auth_require_rate_limit
**Status**: OK
- Better Auth config without `rateLimit` leaves auth endpoints unprotected.
- Severity Warning | TS | 4 tests

### better_auth_require_secure_cookies
**Status**: OK
- Better Auth config missing `useSecureCookies: true` — session cookies transmitted over HTTP.
- Severity Warning | TS | 4 tests

### better_auth_trusted_providers
**Status**: OK
- `accountLinking` enabled without `trustedProviders` allows any OAuth provider to link accounts.
- Severity Warning | TS | 4 tests

### consistent_existence_index_check
**Status**: OK
- Enforce `=== -1` / `!== -1` for index existence checks.
- Severity Warning | TS | 8 tests

### detect_option_rejectunauthorized
**Status**: OK
- `rejectUnauthorized: false` disables TLS certificate validation.
- Severity Error | TS | 4 tests

### drizzle_chunk_large_batch_insert
**Status**: OK
- Drizzle `.values([...])` with a very large array risks exceeding bind-parameter limits.
- Severity Warning | TS | 5 tests

### drizzle_fk_needs_index
**Status**: OK
- Foreign key without an index — FK columns need explicit indexes.
- Severity Warning | TS | 3 tests

### drizzle_no_push_in_production
**Status**: OK
- `drizzle-kit push` is for dev only — use migrations in production/CI.
- Severity Error | TS | 6 tests

### drizzle_no_sql_raw_with_variable
**Status**: OK
- `sql.raw()` with a non-literal argument is a SQL injection vector.
- Severity Error | Text | 5 tests

### drizzle_returning_on_insert_update
**Status**: OK
- Drizzle insert/update without `.returning()` wastes a round-trip on a follow-up SELECT.
- Severity Warning | TS | 6 tests

### drizzle_timestamp_with_timezone
**Status**: OK
- `timestamp('col')` is timezone-ambiguous.
- Severity Warning | TS | 2 tests

### drizzle_zod_prefer_generated_schema
**Status**: OK
- Manual `z.object({})` in a Drizzle schema file duplicates column definitions.
- Severity Warning | Text | 3 tests

### empty_brace_spaces
**Status**: OK
- Do not add spaces between braces.
- Severity Warning | TS/Rust | 8 tests

### enforce_delete_with_where
**Status**: OK
- `db.delete(table)` without a chained `.where(...)` deletes every row in the table.
- Severity Error | TS | 6 tests

### enforce_update_with_where
**Status**: OK
- `db.update(table).set(...)` without `.where(...)` updates every row in the table.
- Severity Error | TS | 5 tests

### explicit_units
**Status**: OK
- Numeric names should include an explicit unit (Ms, Bytes, Kb...).
- Severity Warning | TS/Rust | 12 tests

### exports_last
**Status**: OK
- All `export` declarations must appear at the end of the file.
- Severity Warning | Text | 4 tests

### express_session_require_name
**Status**: OK
- `session({...})` config is missing the `name` property — the default session cookie name is predi...
- Severity Warning | TS | 6 tests

### filename_naming_convention
**Status**: OK
- Filename does not match the expected kebab-case naming convention.
- Severity Warning | Text | 8 tests

### folder_naming_convention
**Status**: OK
- Folder name does not match the expected kebab-case naming convention.
- Severity Warning | Text | 10 tests

### fsd_no_cross_slice_dependency
**Status**: OK
- Feature-Sliced Design: slices at the same layer must not import from each other.
- Severity Warning | TS | 8 tests

### fsd_no_global_store_imports
**Status**: OK
- Lower FSD layers (entities/shared/widgets) must not import the global store directly.
- Severity Warning | TS | 7 tests

### fsd_no_relative_imports
**Status**: OK
- Feature-Sliced Design: relative imports must not traverse across slices or layers.
- Severity Warning | TS | 8 tests

### fsd_no_ui_in_business_logic
**Status**: OK
- Feature-Sliced Design: business logic segments (model/api/lib) must not import from ui/.
- Severity Warning | TS | 7 tests

### hono_cookie_no_httponly
**Status**: OK
- Cookie set without `httpOnly` — accessible to JavaScript (XSS vector).
- Severity Error | TS | 3 tests

### hono_cookie_no_samesite
**Status**: OK
- Cookie without `sameSite` or with `sameSite: 'None'` — vulnerable to CSRF.
- Severity Warning | TS | 3 tests

### hono_cookie_no_secure
**Status**: OK
- Cookie set without `secure` — sent over unencrypted HTTP.
- Severity Warning | TS | 3 tests

### hono_csp_unsafe
**Status**: OK
- `unsafe-inline` or `unsafe-eval` in CSP defeats its purpose.
- Severity Error | TS | 5 tests

### hono_csrf_missing
**Status**: OK
- Mutation routes without CSRF protection.
- Severity Warning | TS | 4 tests

### hono_missing_secure_headers
**Status**: OK
- Hono app without `secureHeaders()` middleware.
- Severity Warning | TS | 4 tests

### hono_secure_headers_disabled
**Status**: OK
- Security header explicitly disabled in `secureHeaders()`.
- Severity Error | TS | 5 tests

### html_no_abstract_roles
**Status**: OK
- Abstract WAI-ARIA roles must not be used on DOM elements.
- Severity Warning | TS | 5 tests

### html_no_aria_hidden_body
**Status**: OK
- `aria-hidden=\
- Severity Warning | TS | 5 tests

### html_no_duplicate_attrs
**Status**: OK
- HTML elements must not declare the same attribute twice.
- Severity Warning | Text | 6 tests

### html_no_duplicate_id
**Status**: OK
- HTML `id` attributes must be unique within a document.
- Severity Warning | Text | 7 tests

### html_no_nested_interactive
**Status**: OK
- Interactive elements must not be nested inside other interactive elements.
- Severity Warning | TS | 10 tests

### html_no_non_scalable_viewport
**Status**: OK
- Viewport meta tag must not disable user scaling (`user-scalable=no`).
- Severity Warning | Text | 6 tests

### html_no_obsolete_tags
**Status**: OK
- Obsolete HTML tags (center, font, marquee, blink, strike, big, tt) and presentational attributes ...
- Severity Warning | Text | 9 tests

### html_no_positive_tabindex
**Status**: OK
- HTML `tabindex` attribute must not be positive — it breaks natural tab order.
- Severity Warning | TS | 4 tests

### html_no_script_style_type
**Status**: OK
- `<script type=\
- Severity Warning | Text | 7 tests

### html_no_skip_heading_levels
**Status**: OK
- Heading levels should not skip (e.g., h1 to h3 without h2).
- Severity Warning | TS | 8 tests

### html_prefer_https
**Status**: OK
- HTML `href`, `src`, and `action` attributes should use `https://` instead of `http://`.
- Severity Warning | Text | 7 tests

### html_require_button_type
**Status**: OK
- `<button>` must have an explicit `type` attribute.
- Severity Warning | TS | 5 tests

### html_require_closing_tags
**Status**: OK
- Non-void HTML tags must be closed with a matching closing tag.
- Severity Warning | Text | 8 tests

### html_require_doctype
**Status**: OK
- HTML files must start with a `<!DOCTYPE html>` declaration.
- Severity Warning | Text | 8 tests

### html_require_explicit_size
**Status**: OK
- `<img>` and `<video>` must declare `width` and `height` to avoid layout shift.
- Severity Warning | TS | 6 tests

### html_require_img_alt
**Status**: OK
- `<img>` elements must declare an `alt` attribute.
- Severity Warning | TS | 4 tests

### html_require_input_label
**Status**: OK
- Form inputs must have accessible labels.
- Severity Warning | TS | 10 tests

### html_require_meta_charset
**Status**: OK
- HTML documents must declare a character encoding via `<meta charset>`.
- Severity Warning | Text | 4 tests

### html_require_title
**Status**: OK
- HTML documents must declare a `<title>` element inside `<head>`.
- Severity Warning | Text | 3 tests

### i18n_json_identical_keys
**Status**: OK
- Translation file is missing keys present in the base locale.
- Severity Warning | Text | 5 tests

### i18n_json_no_nesting
**Status**: OK
- Translation file uses nested objects — use flat keys instead.
- Severity Warning | Text | 6 tests

### i18n_json_valid_message_syntax
**Status**: OK
- ICU message format syntax is invalid in translation file.
- Severity Error | Text | 8 tests

### i18n_no_concat_translation_key
**Status**: OK
- Dynamic `t()` keys built with concatenation or template literals can't be statically extracted.
- Severity Warning | TS | 3 tests

### i18n_no_hardcoded_string_in_jsx
**Status**: OK
- Hardcoded string literals in JSX text content won't be translated.
- Severity Warning | TS | 5 tests

### i18n_no_manual_pluralization
**Status**: OK
- Manual `count === 1 ? singular : plural` ignores CLDR plural rules for non-English languages.
- Severity Warning | TS | 3 tests

### i18n_no_string_concat_with_translation
**Status**: OK
- Concatenating `t()` results breaks word order in RTL and agglutinative languages.
- Severity Warning | Text | 2 tests

### i18n_no_unnecessary_trans_component
**Status**: OK
- `<Trans>` is for interpolating JSX children — use `t()` for plain text.
- Severity Warning | TS | 5 tests

### i18n_prefer_intl_api
**Status**: OK
- `.toLocaleDateString()` without an explicit locale uses the environment default, which varies by ...
- Severity Warning | TS | 3 tests

### i18n_prefer_logical_css_properties
**Status**: OK
- Physical CSS properties break RTL layouts — use logical equivalents.
- Severity Warning | Text | 6 tests

### import_consistent_type_specifier_style
**Status**: OK
- Type-only imports should use top-level `import type` syntax.
- Severity Warning | TS | 3 tests

### import_dedupe
**Status**: OK
- Duplicate named specifiers inside a single import statement.
- Severity Warning | TS | 4 tests

### import_dynamic_import_chunkname
**Status**: OK
- Dynamic imports require a leading `webpackChunkName` comment.
- Severity Warning | TS | 3 tests

### import_no_amd
**Status**: OK
- AMD `require` and `define` calls are forbidden.
- Severity Warning | TS | 3 tests

### import_no_commonjs
**Status**: OK
- CommonJS `require` calls and `module.exports` are forbidden.
- Severity Warning | TS | 3 tests

### import_no_cycle
**Status**: OK
- Circular imports create tight coupling and initialization issues.
- Severity Warning | TS | 4 tests

### import_no_dynamic_require
**Status**: OK
- Calls to `require()` should use string literals.
- Severity Warning | TS | 3 tests

### import_no_empty_named_blocks
**Status**: OK
- Empty named import blocks are forbidden.
- Severity Warning | TS | 3 tests

### import_no_webpack_loader_syntax
**Status**: OK
- Webpack loader syntax in imports is forbidden.
- Severity Warning | TS | 3 tests

### imports_first
**Status**: OK
- Import statements must appear before any other code.
- Severity Warning | TS | 8 tests

### jsdoc_check_property_names
**Status**: OK
- `@property` names must be unique inside a `@typedef` block.
- Severity Warning | Text | 4 tests

### jsdoc_check_tag_names
**Status**: OK
- JSDoc tag names must be known (e.g. `@param`, `@returns`, …).
- Severity Warning | Text | 4 tests

### jsdoc_check_template_names
**Status**: OK
- `@template` names must be referenced somewhere in the block.
- Severity Warning | Text | 4 tests

### jsdoc_check_types
**Status**: OK
- Prefer lowercase primitives in JSDoc types (e.g. `string` over `String`).
- Severity Warning | Text | 5 tests

### jsdoc_check_values
**Status**: OK
- `@version`, `@since`, `@license` must have a valid value.
- Severity Warning | Text | 5 tests

### jsdoc_complete_sentence
**Status**: OK
- JSDoc descriptions must start with a capital letter and end with punctuation.
- Severity Warning | TS | 4 tests

### jsdoc_informative_docs
**Status**: OK
- JSDoc description merely repeats the name of the symbol without adding useful information.
- Severity Warning | Text | 3 tests

### jsdoc_missing_example
**Status**: OK
- Exported function JSDoc must include an @example block.
- Severity Warning | TS | 4 tests

### jsdoc_needs_description
**Status**: OK
- JSDoc block has tags but no description.
- Severity Warning | TS | 5 tests

### jsdoc_reject_any_type
**Status**: OK
- JSDoc uses `*` or `any` as a type, which defeats the purpose of type documentation.
- Severity Warning | Text | 3 tests

### jsdoc_reject_function_type
**Status**: OK
- JSDoc uses bare `Function` or `function` type instead of a specific function signature.
- Severity Warning | Text | 3 tests

### jsdoc_require_hyphen_before_param_description
**Status**: OK
- Separate the `@param` name from its description with a hyphen.
- Severity Warning | Text | 4 tests

### jsdoc_require_next_description
**Status**: OK
- Each @next tag must have a description.
- Severity Warning | Text | 3 tests

### jsdoc_require_param_description
**Status**: OK
- Every `@param` tag must have a description.
- Severity Warning | Text | 4 tests

### jsdoc_require_param_name
**Status**: OK
- Every `@param` tag must name its parameter.
- Severity Warning | Text | 5 tests

### jsdoc_require_property
**Status**: OK
- `@typedef` / `@interface` blocks for object types must declare at least one `@property`.
- Severity Warning | Text | 4 tests

### jsdoc_require_property_description
**Status**: OK
- Each @property tag must have a description.
- Severity Warning | Text | 3 tests

### jsdoc_require_property_name
**Status**: OK
- Each @property tag must have a name.
- Severity Warning | Text | 4 tests

### jsdoc_require_rejects
**Status**: OK
- Async functions that reject must document a @rejects tag.
- Severity Warning | Text | 5 tests

### jsdoc_require_returns_description
**Status**: OK
- `@returns` tag must have a description.
- Severity Warning | Text | 4 tests

### jsdoc_require_tags
**Status**: OK
- Exported function JSDoc must document parameters and return when relevant.
- Severity Warning | Text | 4 tests

### jsdoc_require_template
**Status**: OK
- Generic functions must document each type parameter with @template.
- Severity Warning | Text | 3 tests

### jsdoc_require_template_description
**Status**: OK
- Each @template tag must have a description.
- Severity Warning | Text | 4 tests

### jsdoc_require_yields
**Status**: OK
- Generator functions must document a @yields tag.
- Severity Warning | Text | 3 tests

### jsdoc_require_yields_check
**Status**: OK
- `@yields` must match what the function actually yields.
- Severity Warning | Text | 4 tests

### jsdoc_require_yields_description
**Status**: OK
- Each @yields tag must have a description.
- Severity Warning | Text | 4 tests

### jsdoc_valid_types
**Status**: OK
- JSDoc `{...}` type expressions must be syntactically balanced and non-empty.
- Severity Warning | Text | 5 tests

### jsx_ensure_booleans
**Status**: OK
- Left-hand side of `{x && <Jsx />}` must be an unambiguous boolean.
- Severity Warning | TS | 7 tests

### jsx_no_new_function_as_prop
**Status**: OK
- Arrow/function expressions as JSX prop values create a new reference every render.
- Severity Warning | TS | 6 tests

### justify_inaction
**Status**: OK
- Empty catch/else/match-arm/loop block without an explaining comment inside.
- Severity Warning | TS/Rust/Vue | 43 tests

### migration_needs_lock_timeout
**Status**: OK
- DDL migration without `SET lock_timeout` risks write queue pileups.
- Severity Warning | Text | 2 tests

### migration_needs_rollback
**Status**: OK
- Migration without a `down`/rollback function is irreversible.
- Severity Warning | Text | 3 tests

### mysql_no_multiple_statements
**Status**: OK
- `multipleStatements: true` on mysql connections amplifies SQL injection risk.
- Severity Error | TS | 5 tests

### newline_after_import
**Status**: OK
- Missing blank line after the last import statement.
- Severity Warning | Text | 3 tests

### no_absolute_path
**Status**: OK
- Import uses an absolute path — use relative or aliased paths.
- Severity Warning | Text | 3 tests

### no_abusive_eslint_disable
**Status**: OK
- `eslint-disable` without specifying rules silences everything — too broad.
- Severity Warning | Text | 8 tests

### no_alias_methods
**Status**: OK
- Jest/Vitest alias matchers should be replaced by their canonical form.
- Severity Warning | TS | 8 tests

### no_all_duplicated_branches
**Status**: OK
- All branches have the same implementation — the conditional is pointless.
- Severity Error | TS/Rust | 7 tests

### no_and_in_function_name
**Status**: OK
- `And` in a function name signals two responsibilities — split it.
- Severity Error | TS/Rust | 10 tests

### no_arguments_usage
**Status**: OK
- Direct use of the `arguments` object is discouraged.
- Severity Error | TS | 5 tests

### no_array_callback_reference
**Status**: OK
- Do not pass a function reference directly to an array iterator method.
- Severity Warning | TS | 7 tests

### no_array_constructor
**Status**: OK
- `new Array()` is ambiguous — single numeric arg creates sparse array.
- Severity Error | TS | 5 tests

### no_array_delete
**Status**: OK
- `delete` on an array element creates a sparse hole instead of removing.
- Severity Error | TS | 4 tests

### no_array_method_this_argument
**Status**: OK
- Do not use the `thisArg` parameter in array methods.
- Severity Warning | TS | 6 tests

### no_array_reverse
**Status**: OK
- `Array#reverse()` mutates the array in place.
- Severity Warning | TS | 4 tests

### no_array_sort_mutation
**Status**: OK
- Prefer `Array#toSorted()` over `Array#sort()` (mutates in place).
- Severity Warning | TS | 5 tests

### no_assign_mutated_array
**Status**: OK
- Do not assign the result of a mutating array method (`sort`, `reverse`, `fill`).
- Severity Warning | TS | 9 tests

### no_associative_arrays
**Status**: OK
- Arrays should not be used as associative arrays (use Map or object instead).
- Severity Error | TS | 4 tests

### no_async_without_await
**Status**: OK
- `async` function never uses `await`.
- Severity Warning | TS | 7 tests

### no_await_expression_member
**Status**: OK
- Do not access a member directly from an await expression.
- Severity Warning | TS | 6 tests

### no_hardcoded_secret_signature
**Status**: OK
- Hardcoded secrets in JWT signing or crypto operations leak credentials into source control.
- Severity Error | Text | 5 tests

### no_import_module_exports
**Status**: OK
- File mixes `import` declarations with `module.exports`.
- Severity Warning | TS | 3 tests

### no_import_node_modules_by_path
**Status**: OK
- Importing from a literal `node_modules/` path bypasses the module resolver.
- Severity Warning | TS | 7 tests

### no_import_node_test
**Status**: OK
- Importing from `node:test` alongside vitest/jest mixes test runners.
- Severity Warning | TS | 6 tests

### no_inline_param_type
**Status**: OK
- Inline object types in parameters resist reuse and refactoring.
- Severity Warning | TS | 4 tests

### no_nullish_default_on_input
**Status**: OK
- Defaulting function parameters silently paves over invalid input.
- Severity Warning | TS | 4 tests

### no_unverified_hostname
**Status**: OK
- Disabling TLS hostname verification allows man-in-the-middle attacks.
- Severity Error | TS | 5 tests

### no_useless_collection_argument
**Status**: OK
- Disallow useless values in `Set`, `Map`, `WeakSet`, or `WeakMap` constructors.
- Severity Warning | TS | 7 tests

### no_useless_error_capture_stack_trace
**Status**: OK
- Unnecessary `Error.captureStackTrace()` in Error subclass constructor.
- Severity Warning | TS | 7 tests

### no_useless_fallback_in_spread
**Status**: OK
- Disallow useless fallback when spreading in object literals.
- Severity Warning | TS | 6 tests

### no_useless_increment
**Status**: OK
- `return x++` / `return x--` returns the value *before* the increment.
- Severity Error | TS/Rust | 6 tests

### no_useless_intersection
**Status**: OK
- Intersecting with `any` or `unknown` is useless — `& any` produces `any`, `& unknown` is a no-op.
- Severity Warning | TS | 5 tests

### no_useless_iterator_to_array
**Status**: OK
- Disallow unnecessary `.toArray()` on iterators.
- Severity Warning | TS | 6 tests

### no_useless_length_check
**Status**: OK
- Disallow useless array length check.
- Severity Warning | TS | 6 tests

### no_useless_path_segments
**Status**: OK
- Import paths should not contain useless `/../` or `/./` segments.
- Severity Warning | TS | 7 tests

### no_useless_promise_resolve_reject
**Status**: OK
- Disallow returning `Promise.resolve/reject()` in async functions.
- Severity Warning | TS | 7 tests

### no_useless_react_setstate
**Status**: OK
- Calling a `useState` setter with its own state value is a no-op.
- Severity Warning | TS | 4 tests

### no_useless_switch_case
**Status**: OK
- Disallow useless case in switch statements.
- Severity Warning | TS | 6 tests

### node_callback_return
**Status**: OK
- Callback invocations should be followed by a `return`.
- Severity Warning | TS | 3 tests

### node_global_require
**Status**: OK
- `require()` calls should be at the top-level module scope.
- Severity Warning | TS | 3 tests

### node_handle_callback_err
**Status**: OK
- Callback error parameter is declared but never used.
- Severity Warning | TS | 6 tests

### node_hashbang
**Status**: OK
- Files with a hashbang (`#!`) must use the correct format.
- Severity Warning | Text | 3 tests

### node_no_callback_literal
**Status**: OK
- First argument to error-first callbacks should be an Error object or `null`, not a string literal.
- Severity Warning | TS | 6 tests

### node_no_exports_assign
**Status**: OK
- Direct assignment to `exports` variable is forbidden.
- Severity Error | TS | 3 tests

### node_no_mixed_requires
**Status**: OK
- `require` calls should not be mixed with regular variable declarations.
- Severity Warning | TS | 3 tests

### node_no_new_require
**Status**: OK
- `new require('...')` is almost always a bug.
- Severity Error | TS | 4 tests

### node_no_path_concat
**Status**: OK
- String concatenation with `__dirname` / `__filename` is platform-dependent.
- Severity Warning | TS | 5 tests

### node_no_process_env
**Status**: OK
- Direct use of `process.env` is discouraged.
- Severity Warning | TS | 3 tests

### node_no_sync
**Status**: OK
- Synchronous Node.js methods block the event loop.
- Severity Warning | TS | 4 tests

### node_no_top_level_await
**Status**: OK
- Top-level `await` is forbidden in published modules.
- Severity Error | TS | 3 tests

### node_prefer_promises_dns
**Status**: OK
- Callback-based `dns.*` methods are discouraged.
- Severity Warning | TS | 4 tests

### node_prefer_promises_fs
**Status**: OK
- Callback-based `fs.*` methods are discouraged.
- Severity Warning | TS | 5 tests

### non_existent_operator
**Status**: OK
- Typo operator detected — `=+`, `=-`, `=!` are not valid operators.
- Severity Error | TS/Rust | 8 tests

### number_literal_case
**Status**: OK
- Enforce proper case for numeric literals.
- Severity Warning | TS/Rust | 15 tests

### numeric_separators_style
**Status**: OK
- Enforce the style of numeric separators by correctly grouping digits.
- Severity Warning | TS | 6 tests

### operation_returning_nan
**Status**: OK
- Arithmetic operation will produce `NaN`.
- Severity Error | TS/Rust | 9 tests

### option_vs_result
**Status**: OK
- Functions named `find*`/`get*` returning `null`/`undefined` should use an Option type.
- Severity Warning | TS | 4 tests

### os_command
**Status**: OK
- Detects potential OS command injection via exec/spawn with dynamic input.
- Severity Error | TS | 6 tests

### package_json_sorted_deps
**Status**: OK
- Unsorted dependencies in package.json cause needless merge conflicts.
- Severity Warning | Text | 4 tests

### package_json_unique_deps
**Status**: OK
- A package in both dependencies and devDependencies is ambiguous — \
- Severity Warning | Text | 3 tests

### pg_require_limit
**Status**: OK
- SQL `SELECT` statements without a `LIMIT` clause can return unbounded rows.
- Severity Error | TS | 9 tests

### playwright_expect_expect
**Status**: OK
- Test has no assertions — every test should verify behaviour.
- Severity Warning | TS | 3 tests

### playwright_max_expects
**Status**: OK
- Too many assertions in a single test — split into focused tests.
- Severity Warning | TS | 2 tests

### playwright_max_nested_describe
**Status**: OK
- Deeply nested `describe` blocks reduce readability.
- Severity Warning | TS | 2 tests

### playwright_missing_await
**Status**: OK
- Playwright async method call is missing `await`.
- Severity Error | TS | 4 tests

### playwright_no_commented_out_tests
**Status**: OK
- Commented-out tests are dead code that hides missing coverage.
- Severity Warning | TS | 3 tests

### playwright_no_conditional_expect
**Status**: OK
- `expect()` inside `if`/`switch`/`catch` may silently skip — tests must assert unconditionally.
- Severity Warning | TS | 4 tests

### playwright_no_conditional_in_test
**Status**: OK
- Conditional logic in tests makes them non-deterministic.
- Severity Warning | TS | 3 tests

### playwright_no_duplicate_hooks
**Status**: OK
- Duplicate hooks in a describe block are confusing and error-prone.
- Severity Warning | TS | 2 tests

### playwright_no_element_handle
**Status**: OK
- `page.$()` / `page.$$()` return ElementHandles, which are deprecated in favor of Locators.
- Severity Warning | TS | 4 tests

### playwright_no_eval
**Status**: OK
- `$eval` / `$$eval` evaluate arbitrary code against the DOM — brittle and hard to debug.
- Severity Warning | TS | 4 tests

### playwright_no_force_option
**Status**: OK
- `force: true` bypasses Playwright's actionability checks, hiding real UI issues.
- Severity Warning | TS | 5 tests

### playwright_no_hooks
**Status**: OK
- Hooks add implicit shared state between tests.
- Severity Warning | TS | 3 tests

### playwright_no_nested_step
**Status**: OK
- Nested `test.step()` calls make test flow hard to follow.
- Severity Warning | TS | 2 tests

### playwright_no_networkidle
**Status**: OK
- `networkidle` is fragile — it waits for no network activity for 500 ms, which is race-prone.
- Severity Warning | TS | 4 tests

### playwright_no_nth_methods
**Status**: OK
- `.first()`, `.last()`, `.nth()` create fragile locators.
- Severity Warning | TS | 3 tests

### playwright_no_page_pause
**Status**: OK
- `page.pause()` is a debug-only API that halts test execution.
- Severity Error | TS | 4 tests

### playwright_no_raw_locators
**Status**: OK
- `page.locator('css-selector')` is brittle — prefer `getByRole`, `getByText`, etc.
- Severity Warning | TS | 5 tests

### playwright_no_skipped_test
**Status**: OK
- Skipped tests silently erode coverage.
- Severity Warning | TS | 3 tests

### playwright_no_standalone_expect
**Status**: OK
- `expect()` outside a test body never runs as an assertion.
- Severity Warning | TS | 3 tests

### playwright_no_unsafe_references
**Status**: OK
- `page.evaluate()` runs in the browser — outer-scope variables are not available unless passed as ...
- Severity Warning | TS | 5 tests

### playwright_no_useless_await
**Status**: OK
- Unnecessary `await` on synchronous Playwright methods.
- Severity Warning | TS | 3 tests

### playwright_no_useless_not
**Status**: OK
- Using `.not.toBeVisible()` when `.toBeHidden()` exists is needlessly indirect.
- Severity Warning | TS | 3 tests

### playwright_no_wait_for_navigation
**Status**: OK
- `page.waitForNavigation()` is discouraged — use `waitForURL` instead.
- Severity Warning | TS | 2 tests

### playwright_no_wait_for_selector
**Status**: OK
- `page.waitForSelector()` is discouraged — use web-first assertions.
- Severity Warning | TS | 2 tests

### playwright_no_wait_for_timeout
**Status**: OK
- `page.waitForTimeout()` introduces fragile fixed sleeps in tests.
- Severity Warning | TS | 4 tests

### playwright_prefer_comparison_matcher
**Status**: OK
- Use built-in comparison matchers instead of comparing manually.
- Severity Warning | TS | 3 tests

### playwright_prefer_equality_matcher
**Status**: OK
- Use an equality matcher instead of `expect(a === b).toBe(true)`.
- Severity Warning | TS | 3 tests

### playwright_prefer_hooks_in_order
**Status**: OK
- Hooks should follow the lifecycle order: beforeAll, beforeEach, afterEach, afterAll.
- Severity Warning | TS | 2 tests

### playwright_prefer_hooks_on_top
**Status**: OK
- Hooks should come before any test cases.
- Severity Warning | TS | 2 tests

### playwright_prefer_native_locators
**Status**: OK
- `locator('[role=\
- Severity Warning | TS | 5 tests

### playwright_prefer_strict_equal
**Status**: OK
- Prefer `toStrictEqual()` for more predictable deep equality checks.
- Severity Warning | TS | 3 tests

### playwright_prefer_to_be
**Status**: OK
- Use `toBe()` for primitives — `toEqual` does unnecessary deep comparison.
- Severity Warning | TS | 4 tests

### playwright_prefer_to_contain
**Status**: OK
- Use `toContain()` instead of `expect(arr.includes(x)).toBe(true)`.
- Severity Warning | TS | 3 tests

### playwright_prefer_to_have_count
**Status**: OK
- Prefer `expect(locator).toHaveCount(n)` over `expect(await locator.count()).toBe(n)`.
- Severity Warning | TS | 6 tests

### playwright_prefer_web_first_assertions
**Status**: OK
- `expect(await locator.isVisible()).toBe(true)` does not auto-retry — use web-first assertions.
- Severity Warning | TS | 5 tests

### post_message_origin
**Status**: OK
- Requires explicit target origin in `postMessage()` calls.
- Severity Error | TS | 4 tests

### prefer_array_index_of
**Status**: OK
- Prefer `.indexOf(val)` over `.findIndex(x => x === val)`.
- Severity Warning | TS | 6 tests

### prefer_array_some
**Status**: OK
- Prefer `.some(…)` over `.filter(…).length` checks.
- Severity Warning | TS | 5 tests

### prefer_array_to_reversed
**Status**: OK
- Prefer `arr.toReversed()` over `[...arr].reverse()`.
- Severity Warning | TS | 4 tests

### prefer_array_to_sorted
**Status**: OK
- Prefer `arr.toSorted()` over `[...arr].sort()`.
- Severity Warning | TS | 5 tests

### prefer_at
**Status**: OK
- Prefer `.at()` method for index access and `String#charAt()`.
- Severity Warning | TS | 5 tests

### prefer_bigint_literals
**Status**: OK
- Prefer `BigInt` literals over `BigInt(…)` constructor.
- Severity Warning | TS | 5 tests

### prefer_blob_reading_methods
**Status**: OK
- Prefer `Blob#text()` / `Blob#arrayBuffer()` over `FileReader` methods.
- Severity Warning | TS | 4 tests

### prefer_called_exactly_once_with
**Status**: OK
- Prefer `toHaveBeenCalledExactlyOnceWith(args)` over separate `toHaveBeenCalledTimes(1)` + `toHave...
- Severity Warning | TS | 6 tests

### prefer_called_with
**Status**: OK
- Prefer `toHaveBeenCalledWith(...)` over bare `toHaveBeenCalled()` to assert specific arguments.
- Severity Warning | TS | 4 tests

### prefer_class_fields
**Status**: OK
- Prefer class field declarations over `this` assignments in constructors for static values.
- Severity Warning | TS | 8 tests

### prefer_classlist_toggle
**Status**: OK
- Prefer `Element#classList.toggle()` over conditional `add`/`remove`.
- Severity Warning | TS | 5 tests

### prefer_code_point
**Status**: OK
- Prefer `String#codePointAt()` over `String#charCodeAt()` and `String.fromCodePoint()` over `Strin...
- Severity Warning | TS | 4 tests

### prefer_date_now
**Status**: OK
- Prefer `Date.now()` over `new Date().getTime()`, `+new Date()`, or `Number(new Date())`.
- Severity Warning | TS | 6 tests

### prefer_default_last
**Status**: OK
- `default` clause in switch should be the last clause.
- Severity Warning | TS | 3 tests

### prefer_default_parameters
**Status**: OK
- Prefer default parameters over reassignment.
- Severity Warning | TS | 5 tests

### prefer_destructuring_assignment
**Status**: OK
- Consecutive property accesses on the same object can be destructured.
- Severity Warning | TS | 4 tests

### prefer_dom_node_append
**Status**: OK
- Prefer `Node#append()` over `Node#appendChild()`.
- Severity Warning | TS | 3 tests

### prefer_dom_node_dataset
**Status**: OK
- Prefer `.dataset` over `.setAttribute('data-*')` / `.getAttribute('data-*')`.
- Severity Warning | TS | 4 tests

### prefer_dom_node_remove
**Status**: OK
- Prefer `childNode.remove()` over `parentNode.removeChild(childNode)`.
- Severity Warning | TS | 3 tests

### prefer_dom_node_text_content
**Status**: OK
- Prefer `.textContent` over `.innerText`.
- Severity Warning | TS | 3 tests

### prefer_event_target
**Status**: OK
- Prefer `EventTarget` over `EventEmitter`.
- Severity Warning | TS | 6 tests

### prefer_expect_resolves
**Status**: OK
- Prefer `await expect(promise).resolves` over `expect(await promise)`.
- Severity Warning | TS | 5 tests

### prefer_exponentiation_operator
**Status**: OK
- Prefer `x ** y` over `Math.pow(x, y)`.
- Severity Warning | TS | 4 tests

### prefer_export_from
**Status**: OK
- Prefer `export { x } from './m'` over import-then-re-export.
- Severity Warning | TS | 6 tests

### prefer_global_this
**Status**: OK
- Prefer `globalThis` over `window`, `self`, and `global`.
- Severity Warning | TS | 12 tests

### prefer_immediate_return
**Status**: OK
- Variable is assigned and immediately returned.
- Severity Warning | TS/Rust | 15 tests

### prefer_import_meta_properties
**Status**: OK
- Prefer `import.meta.filename` and `import.meta.dirname` over legacy techniques.
- Severity Warning | TS | 6 tests

### prefer_includes
**Status**: OK
- Prefer `.includes(x)` over `.indexOf(x) !== -1`.
- Severity Warning | TS | 6 tests

### prefer_json_parse_buffer
**Status**: OK
- Prefer reading a JSON file as a buffer.
- Severity Warning | TS | 4 tests

### prefer_keyboard_event_key
**Status**: OK
- Prefer `KeyboardEvent#key` over `KeyboardEvent#keyCode`.
- Severity Warning | TS | 5 tests

### prefer_lazy_load
**Status**: OK
- `<img>` and `<iframe>` should set `loading=\
- Severity Warning | TS | 5 tests

### prefer_less_than
**Status**: OK
- Prefer `<` / `<=` over `>` / `>=` for readability.
- Severity Warning | TS/Rust | 12 tests

### prefer_logical_operator_over_ternary
**Status**: OK
- Prefer `||`/`??` over a ternary that repeats the test in a branch.
- Severity Warning | TS | 5 tests

### prefer_math_min_max
**Status**: OK
- Prefer `Math.min()`/`Math.max()` over comparison ternaries.
- Severity Warning | TS | 8 tests

### prefer_math_trunc
**Status**: OK
- Prefer `Math.trunc(x)` over bitwise hacks like `x | 0`, `~~x`, or `x >> 0`.
- Severity Warning | TS | 5 tests

### prefer_mock_promise_shorthand
**Status**: OK
- Prefer `.mockResolvedValue(x)` / `.mockRejectedValue(x)` over `.mockImplementation(() => Promise....
- Severity Warning | TS | 10 tests

### prefer_mock_return_shorthand
**Status**: OK
- Prefer `.mockReturnValue(x)` over `.mockImplementation(() => x)`.
- Severity Warning | TS | 10 tests

### prefer_modern_dom_apis
**Status**: OK
- Prefer `.before()` / `.replaceWith()` over `.insertBefore()` / `.replaceChild()`.
- Severity Warning | TS | 5 tests

### prefer_modern_math_apis
**Status**: OK
- Prefer modern `Math` APIs: `Math.hypot()`, `Math.log2()`, `Math.log10()`.
- Severity Warning | TS | 6 tests

### prefer_module
**Status**: OK
- Prefer ESM (`import`/`export`) over CommonJS (`require`/`module.exports`).
- Severity Warning | TS | 6 tests

### prefer_native_coercion_functions
**Status**: OK
- Prefer using `String`, `Number`, `BigInt`, `Boolean`, and `Symbol` directly.
- Severity Warning | TS | 9 tests

### prefer_negative_index
**Status**: OK
- Prefer negative index over `.length - index` for `slice`, `splice`, `at`, `with`, and related met...
- Severity Warning | TS | 6 tests

### prefer_node_protocol
**Status**: OK
- Prefer `node:` protocol for Node.js builtin imports.
- Severity Warning | Text | 9 tests

### prefer_number_properties
**Status**: OK
- Prefer `Number.isNaN()`, `Number.parseInt()`, etc. over global equivalents.
- Severity Warning | TS | 8 tests

### prefer_object_from_entries
**Status**: OK
- Prefer `Object.fromEntries()` over building objects from key-value pairs via `reduce`.
- Severity Warning | TS | 5 tests

### prefer_object_has_own
**Status**: OK
- Prefer `Object.hasOwn(obj, key)` over `obj.hasOwnProperty(key)`.
- Severity Warning | TS | 4 tests

### prefer_object_literal
**Status**: OK
- Use `{}` instead of `new Object()`.
- Severity Warning | TS | 4 tests

### prefer_optional_catch_binding
**Status**: OK
- Prefer omitting the `catch` binding parameter when it is unused.
- Severity Warning | TS | 6 tests

### prefer_promise_all
**Status**: OK
- Sequential `await` on independent async calls creates an unnecessary waterfall.
- Severity Warning | TS | 5 tests

### prefer_promise_shorthand
**Status**: OK
- `new Promise` wrapping a single `resolve`/`reject` call — use `Promise.resolve`/`Promise.reject` ...
- Severity Warning | TS | 4 tests

### prefer_prototype_methods
**Status**: OK
- Prefer borrowing methods from the prototype instead of a literal instance.
- Severity Warning | TS | 6 tests

### prefer_query_selector
**Status**: OK
- Prefer `.querySelector()` / `.querySelectorAll()` over legacy DOM query methods.
- Severity Warning | TS | 4 tests

### prefer_reflect_apply
**Status**: OK
- Prefer `Reflect.apply(fn, thisArg, args)` over `fn.apply(thisArg, args)`.
- Severity Warning | TS | 4 tests

### prefer_regexp_exec
**Status**: OK
- `.match(/regex/)` is slower than `regex.exec(string)` for non-global regexps.
- Severity Warning | TS | 4 tests

### prefer_regexp_test
**Status**: OK
- Prefer `RegExp#test()` over `String#match()` in boolean contexts.
- Severity Warning | TS | 5 tests

### prefer_response_static_json
**Status**: OK
- Prefer `Response.json()` over `new Response(JSON.stringify())`.
- Severity Warning | TS | 4 tests

### prefer_set_has
**Status**: OK
-     description:
- Severity Warning | TS | 5 tests

### prefer_set_size
**Status**: OK
- Prefer `Set#size` instead of spreading into an array and reading `.length`.
- Severity Warning | TS | 5 tests

### prefer_single_boolean_return
**Status**: OK
- `if (cond) return true; else return false;` can be replaced by `return cond;`.
- Severity Warning | TS | 9 tests

### prefer_single_call
**Status**: OK
- Combine multiple consecutive `.push()`, `.classList.add()`, or `.classList.remove()` into one call.
- Severity Warning | TS | 5 tests

### prefer_spread
**Status**: OK
- Prefer the spread operator over `Array.from()`, `Array#concat()`, and `Array#slice()`.
- Severity Warning | TS | 6 tests

### prefer_spy_on
**Status**: OK
-     description:
- Severity Warning | TS | 5 tests

### prefer_string_raw
**Status**: OK
- `String.raw` should be used to avoid escaping `\\`.
- Severity Warning | TS | 4 tests

### prefer_string_replace_all
**Status**: OK
- Prefer `String#replaceAll()` over `String#replace()` with a global regex.
- Severity Warning | TS | 6 tests

### prefer_string_slice
**Status**: OK
- Prefer `String#slice()` over `String#substr()` and `String#substring()`.
- Severity Warning | TS | 4 tests

### prefer_string_starts_ends_with
**Status**: OK
- Prefer `String#startsWith()` / `String#endsWith()` over regex `^` / `$` tests.
- Severity Warning | TS | 6 tests

### prefer_string_trim_start_end
**Status**: OK
- Prefer `String#trimStart()` / `String#trimEnd()` over the deprecated `trimLeft()` / `trimRight()`.
- Severity Warning | TS | 6 tests

### prefer_structured_clone
**Status**: OK
-     description:
- Severity Warning | TS | 5 tests

### prefer_switch_over_chained_if
**Status**: OK
- Long if/else-if chains should be switch statements.
- Severity Warning | TS | 4 tests

### prefer_ternary
**Status**: OK
- Simple if/else assignment can be a ternary expression.
- Severity Warning | TS | 9 tests

### prefer_timer_args
**Status**: OK
- Prefer `setTimeout(fn, delay, arg)` over `setTimeout(() => fn(arg), delay)`.
- Severity Warning | TS | 5 tests

### prefer_to_have_length
**Status**: OK
- Use `toHaveLength(n)` instead of asserting on `.length` with `toBe`/`toEqual`.
- Severity Warning | TS | 5 tests

### prefer_todo
**Status**: OK
- Empty test body — use `test.todo` to mark unimplemented tests.
- Severity Warning | TS | 5 tests

### prefer_top_level_await
**Status**: OK
- Prefer top-level await over async IIFE or async-function-then-call patterns.
- Severity Warning | TS | 6 tests

### prefer_type_error
**Status**: OK
- Use `TypeError` instead of `Error` in type-checking conditions.
- Severity Warning | TS | 9 tests

### prefer_type_guard
**Status**: OK
- Functions named `isX` returning `boolean` with `typeof`/`instanceof` should use type predicates.
- Severity Warning | TS | 4 tests

### prefer_type_over_interface
**Status**: OK
- Prefer `type` over `interface` unless you need extension.
- Severity Error | TS | 6 tests

### prefer_url_canparse
**Status**: OK
- Prefer `URL.canParse(url)` over try-catch with `new URL()`.
- Severity Warning | TS | 4 tests

### prefer_while
**Status**: OK
- `for (;;)` or `for (;cond;)` without init/update — use `while` instead.
- Severity Warning | TS/Rust | 7 tests

### proper_arrows_name
**Status**: OK
- Anonymous arrow functions show up as `<anonymous>` in stack traces.
- Severity Warning | TS | 7 tests

### public_static_readonly
**Status**: OK
- `public static` fields without `readonly` allow accidental mutation.
- Severity Warning | TS | 4 tests

### pure_by_default
**Status**: OK
- Function references top-level mutable state.
- Severity Warning | TS/Rust | 6 tests

### react_async_server_action
**Status**: OK
- Server actions (functions with `\
- Severity Error | TS | 4 tests

### react_button_has_type
**Status**: OK
- `<button>` without an explicit `type` attribute defaults to `submit`, which may cause unexpected ...
- Severity Warning | TS/Text | 6 tests

### react_checked_requires_onchange
**Status**: OK
- `checked` prop without `onChange` or `readOnly` makes the input uncontrollable.
- Severity Warning | TS/Text | 8 tests

### react_duplicate_use_directive
**Status**: OK
- A file can have `\
- Severity Error | TS | 3 tests

### react_forward_ref_uses_ref
**Status**: OK
- `forwardRef` component does not use the `ref` parameter.
- Severity Warning | TS | 3 tests

### react_hoist_regex_outside_component
**Status**: OK
- Regex literals inside components are recompiled every render.
- Severity Warning | TS | 3 tests

### react_hoist_static_jsx
**Status**: OK
- JSX with no dynamic content defined inside a component is \
- Severity Warning | TS | 7 tests

### react_hook_form_destructuring_formstate
**Status**: OK
- Accessing `formState.xxx` without destructuring defeats React Hook Form proxy tracking.
- Severity Warning | TS | 4 tests

### react_iframe_missing_sandbox
**Status**: OK
- `<iframe>` without a `sandbox` attribute is a security risk.
- Severity Warning | TS/Text | 6 tests

### react_jsx_no_bind
**Status**: OK
- Arrow functions and `.bind()` in JSX props create a new reference every render.
- Severity Warning | TS | 6 tests

### react_jsx_no_comment_textnodes
**Status**: OK
- Comments placed as JSX text children are rendered as literal text.
- Severity Warning | TS/Text | 6 tests

### react_jsx_no_duplicate_props
**Status**: OK
- Duplicate props in JSX — the last one silently wins.
- Severity Error | TS/Text | 5 tests

### react_jsx_no_jsx_as_prop
**Status**: OK
- JSX elements/fragments passed directly as prop values cause unnecessary re-renders.
- Severity Warning | TS | 6 tests

### react_jsx_no_new_array_as_prop
**Status**: OK
- Array literals as JSX prop values create a new reference every render.
- Severity Warning | TS | 6 tests

### react_jsx_no_new_object_as_prop
**Status**: OK
- Object literals passed directly as JSX props create a new reference every render.
- Severity Warning | TS | 6 tests

### react_jsx_no_script_url
**Status**: OK
- `href=\
- Severity Error | TS/Text | 7 tests

### react_jsx_no_target_blank
**Status**: OK
- `target=\
- Severity Warning | TS/Text | 6 tests

### react_jsx_no_useless_fragment
**Status**: OK
- Unnecessary `<Fragment>` that wraps a single child or nothing.
- Severity Warning | TS | 2 tests

### react_jsx_pascal_case
**Status**: OK
- User-defined JSX components must use PascalCase.
- Severity Warning | TS/Text | 7 tests

### react_jsx_props_no_spread_multi
**Status**: OK
- Same object spread multiple times on a JSX element.
- Severity Warning | TS/Text | 6 tests

### react_layout_requires_children_prop
**Status**: OK
- App Router layouts must accept and render `children`.
- Severity Error | TS | 5 tests

### react_no_access_state_in_setstate
**Status**: OK
- `this.state` inside `setState()` reads stale state.
- Severity Warning | TS | 4 tests

### react_no_adjacent_inline_elements
**Status**: OK
- Adjacent inline elements without whitespace between them.
- Severity Warning | TS/Text | 5 tests

### react_no_and_conditional_jsx
**Status**: OK
- `&&` renders 0/'' when the left operand is falsy-but-not-false.
- Severity Warning | TS | 3 tests

### react_no_array_index_key
**Status**: OK
- Array indices as React keys break on reorder.
- Severity Warning | TS/Text | 4 tests

### react_no_async_client_component
**Status**: OK
- Client components can't be `async` — only server components can.
- Severity Error | TS | 6 tests

### react_no_browser_api_in_server_component
**Status**: OK
- Browser globals (`window`, `document`, `localStorage`) don't exist on the server.
- Severity Error | TS | 6 tests

### react_no_chain_state_updates
**Status**: OK
- A single `useEffect` callback triggers multiple setState calls.
- Severity Warning | TS | 5 tests

### react_no_children_prop
**Status**: OK
- Passing `children` as a prop instead of nesting content.
- Severity Warning | TS | 3 tests

### react_no_class_component_in_server_component
**Status**: OK
- Class components don't render in server components.
- Severity Error | TS | 7 tests

### react_no_client_hook_in_server_component
**Status**: OK
- React hooks can only run in client components.
- Severity Error | TS | 6 tests

### react_no_client_only_in_server_component
**Status**: OK
- `client-only` can't be imported from a server component.
- Severity Error | TS | 3 tests

### react_no_constructed_context_values
**Status**: OK
- `<Provider value={{ ... }}>` creates a new object every render, causing all consumers to re-render.
- Severity Warning | TS | 4 tests

### react_no_cookies_in_layout
**Status**: OK
- `cookies()`/`headers()` in a Next.js layout makes ALL child pages dynamic.
- Severity Error | TS | 4 tests

### react_no_danger_with_children
**Status**: OK
- Using both `dangerouslySetInnerHTML` and `children` on the same element is invalid.
- Severity Error | TS | 4 tests

### react_no_derived_state_in_effect
**Status**: OK
- `useEffect` whose body only calls a state setter derives state — move the derivation to render.
- Severity Warning | TS | 4 tests

### react_no_empty_effect
**Status**: OK
- `useEffect` called with an empty callback body does nothing.
- Severity Warning | TS | 4 tests

### react_no_event_handler_in_server_component
**Status**: OK
- Event handlers (`onClick`, `onChange`, …) can't run in a server component.
- Severity Error | TS | 6 tests

### react_no_find_dom_node
**Status**: OK
- `findDOMNode` is deprecated in React 19 — use refs instead.
- Severity Warning | TS | 5 tests

### react_no_generate_static_params_in_client
**Status**: OK
- Next.js ignores `generateStaticParams` exports from client components.
- Severity Error | TS | 4 tests

### react_no_initialize_state_in_effect
**Status**: OK
- `useEffect` with empty deps that only calls a `setState` is redundant — initialize in `useState` ...
- Severity Warning | TS | 6 tests

### react_no_inline_default_prop
**Status**: OK
- Non-primitive default props in `memo()` create new references every render, breaking memoization.
- Severity Warning | TS | 5 tests

### react_no_invalid_html_attribute
**Status**: OK
- Invalid value in HTML `rel` attribute.
- Severity Warning | TS/Text | 6 tests

### react_no_javascript_urls
**Status**: OK
- Do not use `javascript:` URLs in JSX `href` / `src` / `action`.
- Severity Error | TS | 5 tests

### react_no_metadata_export_in_client
**Status**: OK
- Next.js ignores `metadata` exports from client components.
- Severity Error | TS | 5 tests

### react_no_namespace
**Status**: OK
- Namespaced JSX elements (`<Foo:bar>`) are not supported by React.
- Severity Error | TS/Text | 6 tests

### react_no_next_headers_in_client
**Status**: OK
- `next/headers` is server-only — importing it from a client component throws.
- Severity Error | TS | 5 tests

### react_no_object_in_dep_array
**Status**: OK
-     description:
- Severity Error | TS | 23 tests

### react_no_object_type_as_default_prop
**Status**: OK
- Object/array/function default props create a new reference every render, breaking `React.memo`.
- Severity Warning | TS | 5 tests

### react_no_pass_data_to_parent
**Status**: OK
- `useEffect` that only calls a parent callback to pass data up — lift state instead.
- Severity Warning | TS | 7 tests

### react_no_reset_all_state_on_prop_change
**Status**: OK
- `useEffect` resets multiple states when a prop changes — use a key instead.
- Severity Warning | TS | 7 tests

### react_no_sequential_await_in_component
**Status**: OK
- Sequential `await` of independent calls inside an async React \
- Severity Warning | TS | 7 tests

### react_no_server_only_in_client
**Status**: OK
- `server-only` can't be imported from a client component.
- Severity Error | TS | 4 tests

### react_no_string_refs
**Status**: OK
- String `ref` attributes are deprecated — use `useRef` / callback refs.
- Severity Error | TS | 3 tests

### react_no_this_in_sfc
**Status**: OK
- `this` has no meaning inside a functional component.
- Severity Error | TS | 4 tests

### react_no_typos
**Status**: OK
- Probable typo in React component static property or lifecycle method.
- Severity Error | TS | 3 tests

### react_no_unescaped_entities
**Status**: OK
- Unescaped `>`, `\
- Severity Warning | TS/Text | 5 tests

### react_no_unstable_nested_components
**Status**: OK
- Component defined inside another component causes unmount/remount every render.
- Severity Warning | TS | 4 tests

### react_passive_event_listeners
**Status**: OK
- Scroll/touch/wheel listeners should be passive to avoid blocking the main thread.
- Severity Warning | TS | 4 tests

### react_prefer_react_cache
**Status**: OK
- Module-level async fetchers should be wrapped in `React.cache()` \
- Severity Warning | TS | 8 tests

### react_prefer_use_transition
**Status**: OK
- Replace manual `loading` state with `useTransition` for concurrent-safe async UI.
- Severity Warning | Text | 3 tests

### react_refresh_only_export_components
**Status**: OK
- Non-component exports alongside component exports break React Fast Refresh (HMR).
- Severity Warning | TS | 3 tests

### react_self_closing_comp
**Status**: OK
- Components and HTML elements without children should use self-closing syntax.
- Severity Warning | TS/Text | 6 tests

### react_server_action_requires_auth
**Status**: OK
- Server Actions with mutations must check authentication.
- Severity Warning | Text | 4 tests

### react_server_action_requires_validation
**Status**: OK
- Server Actions with parameters must validate input before use.
- Severity Warning | Text | 4 tests

### react_style_prop_object
**Status**: OK
- The `style` prop expects an object, not a CSS string.
- Severity Error | TS | 4 tests

### react_use_state_initializer_function
**Status**: OK
- Expensive `useState` initial values should use a lazy initializer `() => expr`.
- Severity Warning | TS | 5 tests

### react_use_state_lazy_init
**Status**: OK
- `useState(expensive())` runs on every render.
- Severity Warning | TS | 4 tests

### react_void_dom_elements_no_children
**Status**: OK
- Void HTML elements like `<br>`, `<img>`, `<input>` cannot have children.
- Severity Error | TS/Text | 7 tests

### redundant_type_aliases
**Status**: OK
- `type X = Y` where Y is a single type adds no structure — it's just renaming.
- Severity Warning | TS | 6 tests

### regex_anchor_precedence
**Status**: OK
- Anchor `^` or `$` in alternation may not bind as expected.
- Severity Warning | TS | 8 tests

### regex_complexity
**Status**: OK
- Regex pattern is overly complex (score > 20).
- Severity Warning | TS | 6 tests

### regex_confusing_quantifier
**Status**: OK
- Quantifier is confusing because its minimum is non-zero but the quantified element can match the ...
- Severity Warning | TS | 6 tests

### regex_no_contradiction_with_assertion
**Status**: OK
- Regex contains an assertion that contradicts the pattern around it, making the branch unmatchable.
- Severity Warning | TS | 6 tests

### regex_no_control_chars
**Status**: OK
- Control characters in regex are usually unintended.
- Severity Warning | TS | 7 tests

### regex_no_dupe_disjunctions
**Status**: OK
- Regex contains duplicate alternatives that are redundant.
- Severity Warning | TS | 5 tests

### regex_no_duplicate_chars
**Status**: OK
- Duplicate characters in regex character class are redundant.
- Severity Warning | TS | 9 tests

### regex_no_empty_after_reluctant
**Status**: OK
- Reluctant quantifier followed by end-of-pattern or group is useless.
- Severity Warning | TS | 8 tests

### regex_no_empty_alternative
**Status**: OK
- Empty alternative in regex matches empty string and is likely a mistake.
- Severity Warning | TS | 7 tests

### regex_no_empty_character_class
**Status**: OK
- Empty character class `[]` matches nothing and is likely a mistake.
- Severity Error | TS | 6 tests

### regex_no_empty_group
**Status**: OK
- Empty capturing group `()` is likely a mistake.
- Severity Warning | TS | 5 tests

### regex_no_empty_lookaround
**Status**: OK
- Empty lookaround (`(?=)`, `(?!)`, `(?<=)`, `(?<!)`) always matches or always fails — likely a mis...
- Severity Warning | TS | 8 tests

### regex_no_empty_string_literal_v
**Status**: OK
- Empty string disjunction in a `v`-flag character class is unexpected and likely a mistake.
- Severity Warning | TS | 6 tests

### regex_no_empty_string_match
**Status**: OK
- Regex that matches the empty string used in `.split()` or `.replace()`.
- Severity Warning | TS | 8 tests

### regex_no_escape_backspace
**Status**: OK
- `[\\b]` in a regex matches the backspace character, not a word boundary — this is almost always a...
- Severity Warning | TS | 5 tests

### regex_no_extra_lookaround_assertions
**Status**: OK
- Lookaround assertion is useless and can be inlined into the parent pattern.
- Severity Warning | TS | 6 tests

### regex_no_invisible_character
**Status**: OK
- Invisible Unicode characters in regex (zero-width joiners, soft hyphens, etc.) are hard to spot a...
- Severity Warning | TS | 7 tests

### regex_no_legacy_features
**Status**: OK
- Regex uses legacy RegExp static properties like `RegExp.$1` or `RegExp.lastMatch`.
- Severity Warning | TS | 7 tests

### regex_no_misleading_capturing_group
**Status**: OK
- Capturing group matches different things at the start and end, which is misleading.
- Severity Warning | TS | 6 tests

### regex_no_misleading_char_class
**Status**: OK
- Character class contains multi-codepoint graphemes that will be split.
- Severity Warning | TS | 7 tests

### regex_no_missing_g_flag
**Status**: OK
- Regex used with a method that expects the global flag but the g flag is missing.
- Severity Warning | TS | 8 tests

### regex_no_multiple_spaces
**Status**: OK
- Multiple consecutive spaces in regex are hard to read and count.
- Severity Warning | TS | 7 tests

### regex_no_non_standard_flag
**Status**: OK
- Regex uses a non-standard flag that is not part of the ECMAScript specification.
- Severity Warning | TS | 7 tests

### regex_no_obscure_range
**Status**: OK
- Character class ranges like `[A-z]` include unwanted chars (`[\\]^_\\``). Use `[A-Za-z]` instead.
- Severity Warning | TS | 7 tests

### regex_no_octal
**Status**: OK
- Octal escapes in regex (`\\1`, `\\12`) are ambiguous — they could be backreferences or octal char...
- Severity Warning | TS | 8 tests

### regex_no_optional_assertion
**Status**: OK
- Assertion inside an optional group is effectively ignored and does not change the pattern.
- Severity Warning | TS | 6 tests

### regex_no_potentially_useless_backreference
**Status**: OK
- Backreference may be useless because some paths to it do not go through the referenced group.
- Severity Warning | TS | 5 tests

### regex_no_single_char_class
**Status**: OK
- Character class with a single character is unnecessary.
- Severity Warning | TS | 8 tests

### regex_no_slow_pattern
**Status**: OK
- Regex has nested quantifiers that can cause catastrophic backtracking (ReDoS).
- Severity Warning | TS | 8 tests

### regex_no_standalone_backslash
**Status**: OK
- Backslash followed by a non-special character in regex is an identity escape — likely a mistake.
- Severity Warning | TS | 8 tests

### regex_no_stateful_global
**Status**: OK
- Global regex used with `.test()` or `.exec()` is stateful via `lastIndex`.
- Severity Warning | TS | 7 tests

### regex_no_super_linear_move
**Status**: OK
- Regex quantifier can cause quadratic runtime on certain inputs.
- Severity Warning | TS | 6 tests

### regex_no_trivially_nested_assertion
**Status**: OK
- Lookaround assertion is trivially nested inside another lookaround of the same kind.
- Severity Warning | TS | 6 tests

### regex_no_trivially_nested_quantifier
**Status**: OK
- Two quantifiers are trivially nested and can be replaced with a single quantifier.
- Severity Warning | TS | 6 tests

### regex_no_unused_groups
**Status**: OK
- Named capturing group is defined but never referenced.
- Severity Warning | TS | 10 tests

### regex_no_useless_assertions
**Status**: OK
- Regex contains an assertion that is always true or always false, making it useless.
- Severity Warning | TS | 9 tests

### regex_no_useless_backreference
**Status**: OK
- Backreference is always replaced by the empty string because it references itself or a group that...
- Severity Warning | TS | 6 tests

### regex_no_useless_dollar_replacements
**Status**: OK
- Replacement string references a capturing group that does not exist in the regex.
- Severity Warning | TS | 12 tests

### regex_no_useless_flag
**Status**: OK
- Regex flag has no effect because the pattern does not contain anything that would be affected by it.
- Severity Warning | TS | 7 tests

### regex_no_useless_lazy
**Status**: OK
- Lazy quantifier has no effect when the quantified token can only match a single length.
- Severity Warning | TS | 7 tests

### regex_no_useless_quantifier
**Status**: OK
- Quantifier can only match once or matches an element that is empty, making it useless.
- Severity Warning | TS | 7 tests

### regex_no_useless_set_operand
**Status**: OK
- Character class set operation has a useless operand that does not affect the result.
- Severity Warning | TS | 6 tests

### regex_no_useless_string_literal
**Status**: OK
- String disjunction of single characters in a `v`-flag character class can be simplified.
- Severity Warning | TS | 6 tests

### regex_no_useless_two_nums_quantifier
**Status**: OK
- Quantifier `{n,n}` is equivalent to `{n}` — the range is redundant.
- Severity Warning | TS | 7 tests

### regex_no_zero_quantifier
**Status**: OK
- Quantifier `{0}` or `{0,0}` matches nothing — the pattern is likely a mistake.
- Severity Warning | TS | 7 tests

### regex_optimal_lookaround_quantifier
**Status**: OK
- Quantified expression at the edge of a lookaround should only match a constant number of times.
- Severity Warning | TS | 6 tests

### regex_prefer_char_class
**Status**: OK
- Single-character alternations should use a character class.
- Severity Warning | TS | 7 tests

### regex_prefer_predefined_assertion
**Status**: OK
- Lookaround assertion can be replaced with a simpler predefined assertion like `\\b` or `^`/`$`.
- Severity Warning | TS | 6 tests

### regex_prefer_quantifier
**Status**: OK
- Repeated identical characters or escape sequences in regex should use quantifiers.
- Severity Warning | TS | 7 tests

### regex_prefer_set_operation
**Status**: OK
- Lookaround combined with a character can be expressed more clearly using a set operation.
- Severity Warning | TS | 6 tests

### regex_sort_flags
**Status**: OK
- Regex flags should be alphabetically sorted for consistency (`dgimsvy`).
- Severity Warning | TS | 8 tests

### regex_use_unicode_flag
**Status**: OK
- Unicode property escapes (`\\p{...}` / `\\P{...}`) require the `u` or `v` flag.
- Severity Warning | TS | 8 tests

### relative_url_style
**Status**: OK
- Remove the `./` prefix from relative URLs in `new URL()`.
- Severity Warning | TS | 5 tests

### require_array_join_separator
**Status**: OK
- Enforce using the separator argument with `Array#join()`.
- Severity Warning | TS | 4 tests

### require_explicit_undefined
**Status**: OK
- Functions that return a value must use `return undefined;` — bare `return;` hides intent.
- Severity Warning | TS | 10 tests

### require_hook
**Status**: OK
- Side effects at the top level of a test file run once at import time instead of inside a hook — t...
- Severity Warning | TS | 9 tests

### require_module_attributes
**Status**: OK
- Import/export with empty attribute list `with {}` is not allowed.
- Severity Warning | TS | 4 tests

### require_module_specifiers
**Status**: OK
- Import/export statements with empty specifier lists are not allowed.
- Severity Warning | TS | 5 tests

### require_not_empty
**Status**: OK
- Module specifiers must not be empty strings.
- Severity Error | TS | 5 tests

### require_number_to_fixed_digits_argument
**Status**: OK
- Enforce using the digits argument with `Number#toFixed()`.
- Severity Warning | TS | 4 tests

### require_path_exists
**Status**: OK
- Relative imports must point to files that exist.
- Severity Error | TS | 2 tests

### require_post_message_target_origin
**Status**: OK
- `postMessage()` called without the `targetOrigin` argument.
- Severity Warning | TS | 5 tests

### require_to_throw_message
**Status**: OK
- Require an expected error message argument on `.toThrow()` / `.toThrowError()`.
- Severity Warning | TS | 5 tests

### require_too_many_arguments
**Status**: OK
- `require()` accepts only one argument; extra arguments are ignored.
- Severity Warning | TS | 5 tests

### rust_anyhow_context_on_question_mark
**Status**: OK
- `?` without `.context()` produces bare error messages with no callsite information.
- Severity Warning | Rust | 3 tests

### rust_arc_non_send_sync
**Status**: OK
- `Arc<T>` where `T: !Send + !Sync` cannot cross threads.
- Severity Error | (delegated) | 0 tests

### rust_await_holding_lock
**Status**: OK
- Never hold a MutexGuard across an `.await` point.
- Severity Error | (delegated) | 0 tests

### rust_block_on_in_async
**Status**: OK
- `block_on` from inside `async fn` panics the runtime.
- Severity Error | Rust | 3 tests

### rust_builder_without_must_use
**Status**: OK
- Builder types need `#[must_use]` to catch forgotten `.build()` calls.
- Severity Warning | Rust | 3 tests

### rust_constants_top_of_file
**Status**: OK
- Module-level `const` / `static` must appear before any `fn` / `struct` / `impl`.
- Severity Warning | Rust | 8 tests

### rust_duration_over_integer_with_unit
**Status**: OK
- Prefer `Duration` over integers whose name encodes a time unit.
- Severity Warning | Rust | 8 tests

### rust_explicit_enum_match_arms
**Status**: OK
- Wildcard `_` arm on a `match` that looks like it covers an enum.
- Severity Warning | Rust | 9 tests

### rust_explicit_iter_loop
**Status**: OK
- Use iterator chains, not raw index loops.
- Severity Warning | (delegated) | 0 tests

### rust_impl_debug_on_public_types
**Status**: OK
- Public structs and enums must derive `Debug`.
- Severity Warning | Rust | 8 tests

### rust_large_enum_variant
**Status**: OK
- Enum size equals the largest variant — box big variants.
- Severity Warning | (delegated) | 0 tests

### rust_mod_tests_without_cfg_test
**Status**: OK
- `mod tests` must be gated by `#[cfg(test)]`.
- Severity Error | Rust | 5 tests

### rust_must_use_on_result_fn
**Status**: OK
- Public functions returning `Result` should be `#[must_use]` so callers can't silently discard err...
- Severity Warning | Rust | 4 tests

### rust_no_as_numeric_cast
**Status**: OK
- Ban every `as` cast whose target is a numeric primitive.
- Severity Warning | Rust | 8 tests

### rust_no_bool_return_from_fallible
**Status**: OK
- Action functions return `Result`, not `bool`.
- Severity Warning | Rust | 6 tests

### rust_no_box_default
**Status**: OK
- `Box::new(T::default())` is `Box::<T>::default()`.
- Severity Warning | (delegated) | 0 tests

### rust_no_dbg_macro
**Status**: OK
- `dbg!()` is a debugging aid that must not ship.
- Severity Error | Rust | 3 tests

### rust_no_empty_test_fn
**Status**: OK
- `#[test] fn x() {}` is a passing stub that exercises nothing.
- Severity Error | Rust | 11 tests

### rust_no_float_for_money
**Status**: OK
- Money fields must not be `f32`/`f64` — IEEE 754 rounding errors corrupt totals.
- Severity Error | Rust | 6 tests

### rust_no_format_in_debug_impl
**Status**: OK
- `format!` inside `Debug::fmt` allocates an extra `String` per call.
- Severity Warning | Rust | 3 tests

### rust_no_large_tuple_return
**Status**: OK
- Function return tuples with 3+ elements should be named structs.
- Severity Warning | Rust | 4 tests

### rust_no_linkedlist
**Status**: OK
- Prefer `Vec<T>` over `LinkedList<T>` — cache locality wins.
- Severity Warning | (delegated) | 0 tests

### rust_no_lossy_as_cast
**Status**: OK
- `as` casts that can truncate or lose precision are silent bugs.
- Severity Warning | Rust | 4 tests

### rust_no_mutex_in_single_threaded
**Status**: OK
- `Mutex<T>` outside of `Arc<Mutex<T>>` is usually a `RefCell<T>` — no thread sharing means no reas...
- Severity Warning | Rust | 7 tests

### rust_no_panic_macros
**Status**: OK
- No `panic!` / `todo!` / `unimplemented!` / `unreachable!` in production.
- Severity Error | Rust | 10 tests

### rust_no_println_in_library
**Status**: OK
- Library code must use tracing, not `println!` / `eprintln!`.
- Severity Error | Rust | 4 tests

### rust_no_pub_use_glob
**Status**: OK
- `pub use foo::*` re-exports invisibly.
- Severity Warning | Rust | 4 tests

### rust_no_static_mut
**Status**: OK
- `static mut` is deprecated and unsafe by design.
- Severity Error | Rust | 3 tests

### rust_no_unwrap_in_from_impl
**Status**: OK
- `From::from` must be infallible — no `.unwrap()` / `.expect()`.
- Severity Error | Rust | 4 tests

### rust_prefer_channel_over_arc_mutex_vec
**Status**: OK
- `Arc<Mutex<Vec<` for collecting task results adds contention. Use `mpsc::channel` instead.
- Severity Warning | Rust | 3 tests

### rust_prefer_cow
**Status**: OK
- Public functions taking an owned `String` force callers to allocate — prefer `Cow<'_, str>` or `&...
- Severity Warning | Rust | 8 tests

### rust_prefer_fast_hasher
**Status**: OK
- `HashMap` / `HashSet` with integer keys defaults to the slower SipHash — use a faster hasher.
- Severity Warning | Rust | 5 tests

### rust_prefer_once_lock
**Status**: OK
- `lazy_static!` and `once_cell` are superseded by `std::sync::OnceLock`/`LazyLock` (Rust 1.70+).
- Severity Warning | Rust | 4 tests

### rust_prefer_unwrap_or_explicit
**Status**: OK
- Ban `.unwrap_or_default()`; require an explicit fallback value.
- Severity Warning | Rust | 7 tests

### rust_ptr_arg
**Status**: OK
- Prefer borrowed slices over borrowed owned types.
- Severity Warning | (delegated) | 0 tests

### rust_pub_enum_without_non_exhaustive
**Status**: OK
- `pub enum` without `#[non_exhaustive]` makes new variants a breaking change.
- Severity Warning | Rust | 3 tests

### rust_rc_mutex
**Status**: OK
- `Rc<Mutex<T>>` pays the Mutex cost for zero benefit — Rc is !Send.
- Severity Error | Rust | 5 tests

### rust_redundant_clone
**Status**: OK
- Remove `.clone()` calls whose result isn't independently observed.
- Severity Warning | (delegated) | 0 tests

### rust_serde_deny_unknown_fields
**Status**: OK
- Deserialize-derive structs need `#[serde(deny_unknown_fields)]`.
- Severity Warning | Rust | 5 tests

### rust_string_as_error
**Status**: OK
- `Result<T, String>` is stringly-typed and unmatchable.
- Severity Warning | Rust | 3 tests

### rust_sync_io_in_async
**Status**: OK
- Synchronous I/O calls inside `async fn` block the runtime.
- Severity Error | Rust | 4 tests

### rust_thiserror_for_lib
**Status**: OK
- Library error types should derive `thiserror::Error` instead of manually implementing `Display`.
- Severity Warning | Rust | 3 tests

### rust_thread_sleep_in_async
**Status**: OK
- `std::thread::sleep` from `async fn` blocks the runtime.
- Severity Error | Rust | 3 tests

### rust_tokio_spawn_without_handle
**Status**: OK
- `tokio::spawn(..)` whose JoinHandle is dropped silently swallows panics.
- Severity Warning | Rust | 4 tests

### rust_unbounded_channel
**Status**: OK
- Unbounded channels can OOM the process.
- Severity Error | Rust | 6 tests

### rust_undocumented_unsafe
**Status**: OK
- Every `unsafe` block must have a `// SAFETY:` comment.
- Severity Error | Rust | 4 tests

### rust_unit_error_result
**Status**: OK
- `Result<T, ()>` discards every error detail.
- Severity Warning | Rust | 4 tests

### rust_unsafe_ffi_isolation
**Status**: OK
- `extern \
- Severity Warning | Rust | 3 tests

### rust_unsafe_impl_without_comment
**Status**: OK
- `unsafe impl` requires a `// SAFETY:` comment.
- Severity Error | Rust | 3 tests

### rust_vec_with_capacity
**Status**: OK
- `Vec::new()` followed by a for-loop with `.push()` reallocates repeatedly. Use `Vec::with_capacit...
- Severity Warning | Rust | 3 tests

### serialize_javascript_no_unsafe
**Status**: OK
- `serialize(value, { unsafe: true })` disables HTML escaping (XSS risk).
- Severity Error | TS | 6 tests

### sql_advisory_lock_prefer_xact
**Status**: OK
- `pg_advisory_lock` holds until session ends, leaking if the connection is reused. Use `pg_advisor...
- Severity Warning | Text | 3 tests

### sql_create_index_concurrently
**Status**: OK
- `CREATE INDEX` without `CONCURRENTLY` takes an `ACCESS EXCLUSIVE` lock, blocking all table access.
- Severity Warning | Text | 4 tests

### sql_index_needs_rationale_comment
**Status**: OK
- `CREATE INDEX` without a SQL comment explaining why the index exists.
- Severity Warning | TS/Rust | 9 tests

### sql_no_between_timestamp
**Status**: OK
- `BETWEEN` with timestamps causes off-by-one bugs (inclusive both sides).
- Severity Warning | TS/Rust/Vue | 21 tests

### sql_no_float_for_money
**Status**: OK
- `FLOAT`/`DOUBLE`/`REAL` near monetary columns — use `NUMERIC` for money.
- Severity Error | Text | 2 tests

### sql_no_like_wildcard_prefix
**Status**: OK
- `LIKE '%...'` prevents index usage — use full-text search instead.
- Severity Warning | Text | 2 tests

### sql_no_offset_pagination
**Status**: OK
- `OFFSET` pagination is O(N) on deep pages — use cursor-based (keyset) pagination.
- Severity Warning | TS/Rust/Vue | 21 tests

### sql_no_pg_enum
**Status**: OK
- PostgreSQL `CREATE TYPE ... AS ENUM` — can't remove values once added.
- Severity Error | Text | 2 tests

### sql_no_select_star
**Status**: OK
- `SELECT *` wastes bandwidth and prevents covering indexes.
- Severity Warning | Text | 3 tests

### sql_no_varchar
**Status**: OK
- `VARCHAR(N)` / `CHAR(N)` provides no perf benefit in PostgreSQL — use `TEXT` with a CHECK constra...
- Severity Error | TS/Rust/Vue | 20 tests

### sql_nullable_requires_comment
**Status**: OK
- Nullable columns must have a `--` comment explaining why NULL is allowed.
- Severity Warning | Text | 4 tests

### sql_prefer_exists_over_in
**Status**: OK
- `WHERE x IN (SELECT ...)` — prefer `EXISTS` which exits on first match.
- Severity Warning | Text | 3 tests

### sql_require_transaction_timeout
**Status**: OK
- DB connection pool config should set `statement_timeout` and `idle_in_transaction_session_timeout...
- Severity Warning | Text | 3 tests

### strings_comparison
**Status**: OK
- Relational comparison with string literals uses lexicographic order.
- Severity Warning | TS/Rust | 9 tests

### structured_api_error
**Status**: OK
- Bare `new Error()` in route handlers — use structured errors.
- Severity Warning | TS/Rust | 5 tests

### switch_case_braces
**Status**: OK
- Missing braces in `case` clause.
- Severity Warning | TS | 5 tests

### switch_case_break_position
**Status**: OK
- `break`/`return` should be inside the case block, not after it.
- Severity Warning | TS | 5 tests

### symmetric_pairs
**Status**: OK
- Exported function has no symmetric counterpart (get/set, add/remove, open/close, start/stop, crea...
- Severity Warning | TS/Rust/Text | 20 tests

### tailwind_classnames_order
**Status**: OK
- Tailwind classes should follow a canonical category order (layout → spacing → sizing → typography...
- Severity Warning | Text | 7 tests

### tailwind_enforces_negative_arbitrary_values
**Status**: OK
- Negative arbitrary Tailwind values should live inside the brackets, not on the utility prefix.
- Severity Warning | Text | 6 tests

### tailwind_no_apply_for_variants
**Status**: OK
- `@apply` outside `@layer base` defeats Tailwind's purging and specificity model.
- Severity Warning | Text | 3 tests

### tailwind_no_arbitrary_z_index
**Status**: OK
- Arbitrary z-index values `z-[n]` bypass the design token scale.
- Severity Warning | Text | 4 tests

### tailwind_no_conflicting_classes
**Status**: OK
- Mutually exclusive Tailwind classes produce unpredictable styles.
- Severity Warning | Text | 5 tests

### tailwind_no_deprecated_classes
**Status**: OK
- Deprecated Tailwind v2/v3 utility classes should be replaced by their v3/v4 equivalents.
- Severity Warning | Text | 6 tests

### tailwind_no_duplicate_classes
**Status**: OK
- Duplicate CSS classes in className/class attributes are redundant and confusing.
- Severity Warning | Text | 4 tests

### tailwind_no_dynamic_class
**Status**: OK
- Dynamic Tailwind classes are purged from the stylesheet.
- Severity Warning | TS | 4 tests

### tailwind_no_important_modifier
**Status**: OK
- The Tailwind `!` important modifier signals a specificity fight, not a real fix.
- Severity Warning | Text | 4 tests

### tailwind_no_magic_spacing
**Status**: OK
- Arbitrary pixel spacing like `p-[13px]` breaks design-token consistency.
- Severity Warning | Text | 7 tests

### tailwind_no_unnecessary_whitespace
**Status**: OK
- Multiple consecutive spaces in className/class attributes are unnecessary.
- Severity Warning | Text | 5 tests

### tailwind_prefer_cn_utility
**Status**: OK
- Ternary or concatenation in `className` should use `cn()` or `clsx()` for readability.
- Severity Warning | TS | 3 tests

### tailwind_prefer_shorthand
**Status**: OK
- Collapse redundant Tailwind utility pairs into their shorthand form (e.g. `px-2 py-2` → `p-2`).
- Severity Warning | Text | 8 tests

### tailwind_prefer_size_shorthand
**Status**: OK
- `w-X h-X` with equal values can be written as `size-X`.
- Severity Warning | Text | 4 tests

### tailwind_read_theme_before_classes
**Status**: OK
- Arbitrary Tailwind values (`p-[13px]`, `bg-[#abc]`) are used without \
- Severity Warning | TS | 7 tests

### tanstack_query_array_key
**Status**: OK
- TanStack Query keys must be arrays, not strings.
- Severity Error | TS | 2 tests

### tanstack_query_fn_must_throw_on_error
**Status**: OK
- `queryFn` must throw on HTTP errors so TanStack Query can retry and surface them.
- Severity Warning | Text | 2 tests

### tanstack_query_key_includes_params
**Status**: OK
- `queryKey` must include every non-parameter identifier referenced by `queryFn`.
- Severity Error | TS | 10 tests

### tanstack_query_no_cache_time
**Status**: OK
- `cacheTime` was renamed to `gcTime` in TanStack Query v5.
- Severity Warning | Text | 2 tests

### tanstack_query_no_deprecated_props
**Status**: OK
- Deprecated TanStack Query props from v4.
- Severity Error | TS | 4 tests

### tanstack_query_no_enabled_true
**Status**: OK
- `enabled: true` is the default in TanStack Query and should be omitted.
- Severity Warning | Text | 3 tests

### tanstack_query_no_is_loading
**Status**: OK
- `isLoading` was renamed to `isPending` in TanStack Query v5.
- Severity Warning | Text | 3 tests

### tanstack_query_no_keep_previous_data_prop
**Status**: OK
- `keepPreviousData: true` was replaced by `placeholderData: keepPreviousData` in v5.
- Severity Warning | Text | 2 tests

### tanstack_query_no_query_callbacks
**Status**: OK
- `onSuccess`/`onError`/`onSettled` callbacks on `useQuery` were removed in v5.
- Severity Warning | Text | 3 tests

### tanstack_query_no_use_error_boundary
**Status**: OK
- `useErrorBoundary` was removed in TanStack Query v5.
- Severity Warning | Text | 2 tests

### tanstack_query_prefer_key_factory
**Status**: OK
- Inline dynamic `queryKey` arrays should use a key factory for consistency.
- Severity Warning | Text | 3 tests

### tanstack_query_prefer_query_options
**Status**: OK
- Inline `queryKey`/`queryFn` objects should be extracted to `queryOptions()` factories for reuse.
- Severity Warning | Text | 2 tests

### tanstack_query_prefer_suspense_query
**Status**: OK
- `useQuery` followed by `if (isLoading|isPending) return …` should use `useSuspenseQuery` instead.
- Severity Warning | TS | 7 tests

### tanstack_query_require_stale_time
**Status**: OK
- `QueryClient` without a default `staleTime` refetches on every mount.
- Severity Warning | Text | 2 tests

### tanstack_start_loader_stale_time
**Status**: OK
- Loader `staleTime` too short — data will refetch during navigation.
- Severity Warning | Text | 6 tests

### tanstack_start_no_client_import_in_server_fn
**Status**: OK
- Client-only React imports in a `.functions.ts` file — server functions cannot use browser APIs.
- Severity Error | Text | 5 tests

### tanstack_start_require_validate_search
**Status**: OK
- Routes calling `Route.useSearch()` must define `validateSearch:` on the route.
- Severity Warning | Text | 3 tests

### tanstack_start_server_fn_file_convention
**Status**: OK
- `createServerFn` must live in a `.functions.ts` file to enforce server/client separation.
- Severity Warning | Text | 3 tests

### tanstack_start_server_fn_requires_auth
**Status**: OK
- `createServerFn` handlers with DB mutations must verify authentication.
- Severity Warning | Text | 3 tests

### tanstack_start_server_fn_requires_validation
**Status**: OK
- `createServerFn` handlers must validate their input with `.input()` or `.safeParse()`.
- Severity Warning | Text | 3 tests

### template_indent
**Status**: OK
- Template literals should not inherit indentation from surrounding code.
- Severity Warning | Text | 5 tests

### test_check_exception
**Status**: OK
- `.toThrow()` without specifying what to check.
- Severity Warning | TS | 4 tests

### testing_no_and_in_test_name
**Status**: OK
- Test names containing \
- Severity Warning | TS | 2 tests

### testing_no_real_external_service
**Status**: OK
- Test makes a real network call to an external service — intercept it with MSW instead.
- Severity Error | TS | 6 tests

### testing_no_undefined_mock_var
**Status**: OK
- `jest.fn()` / `vi.fn()` stored in a variable but never configured with `mockReturnValue` / `mockR...
- Severity Warning | TS | 7 tests

### testing_prefer_msw
**Status**: OK
- Mocking HTTP clients directly is brittle — use MSW to intercept at the network layer.
- Severity Warning | TS | 8 tests

### testing_prefer_test_each
**Status**: OK
- Looping over `test` / `it` hides failures — use `test.each` so each row is its own named case.
- Severity Warning | TS | 6 tests

### text_encoding_identifier_case
**Status**: OK
- Enforce consistent case for text encoding identifiers (`utf-8`, `ascii`).
- Severity Warning | TS | 5 tests

### throw_error_values
**Status**: OK
- Only throw `Error` instances, not primitives or plain objects.
- Severity Warning | TS | 10 tests

### throw_new_error
**Status**: OK
- Use `new` when creating an error.
- Severity Warning | TS | 10 tests

### timeout_on_io
**Status**: OK
- I/O calls without a timeout can hang forever.
- Severity Warning | TS/Rust | 10 tests

### toml_keys_order
**Status**: OK
- Keys inside a TOML table should be declared in alphabetical order.
- Severity Warning | Text | 7 tests

### toml_no_mixed_type_in_array
**Status**: OK
- TOML arrays should contain elements of a single type.
- Severity Warning | (delegated) | 0 tests

### toml_tables_order
**Status**: OK
- Top-level TOML tables should be declared in alphabetical order.
- Severity Warning | Text | 5 tests

### too_many_break_or_continue
**Status**: OK
- Loop contains 2+ `break`/`continue` statements — consider refactoring.
- Severity Warning | TS/Rust | 8 tests

### top_level_function
**Status**: OK
- Top-level arrow-function variables hide their name in stack traces and \
- Severity Warning | TS | 7 tests

### try_catch_json_parse
**Status**: OK
- `JSON.parse` can throw — wrap it in try/catch or a Result helper.
- Severity Warning | TS | 5 tests

### try_catch_new_url
**Status**: OK
- `new URL(...)` can throw — wrap it in try/catch or use `URL.canParse`.
- Severity Warning | TS | 4 tests

### ts_adjacent_overload_signatures
**Status**: OK
- Function overload signatures must be consecutive for readability.
- Severity Warning | TS | 3 tests

### ts_ban_ts_comment
**Status**: OK
- `@ts-ignore` and `@ts-nocheck` suppress compiler errors and hide bugs.
- Severity Warning | TS | 4 tests

### ts_ban_tslint_comment
**Status**: OK
- TSLint comments are obsolete — the project has been deprecated in favour of ESLint.
- Severity Warning | TS | 4 tests

### ts_class_literal_property_style
**Status**: OK
- Enforce that literals on classes are exposed in a consistent style (fields vs getters).
- Severity Warning | TS | 4 tests

### ts_class_methods_use_this
**Status**: OK
- Class methods that don't use `this` should be static or extracted to a standalone function.
- Severity Warning | TS | 4 tests

### ts_consistent_generic_constructors
**Status**: OK
- Generic type arguments should be on the constructor, not the variable annotation.
- Severity Warning | TS | 3 tests

### ts_consistent_indexed_object_style
**Status**: OK
- Prefer `Record<K, V>` over manual index signature `{ [key: K]: V }` for consistency.
- Severity Warning | TS | 3 tests

### ts_consistent_type_assertions
**Status**: OK
- Enforce consistent type assertion style (`as T` vs `<T>`).
- Severity Warning | TS | 2 tests

### ts_consistent_type_exports
**Status**: OK
- Type-only exports should use `export type` rather than `export`.
- Severity Warning | TS | 5 tests

### ts_consistent_type_imports
**Status**: OK
- Type-only imports should use `import type` rather than `import`.
- Severity Warning | TS | 5 tests

### ts_default_param_last
**Status**: OK
- Default parameters should be last to allow callers to omit them positionally.
- Severity Warning | TS | 3 tests

### ts_explicit_function_return_type
**Status**: OK
- Require explicit return types on functions and class methods.
- Severity Warning | TS | 5 tests

### ts_explicit_member_accessibility
**Status**: OK
- Require explicit accessibility modifiers on class properties and methods.
- Severity Warning | TS | 6 tests

### ts_explicit_module_boundary_types
**Status**: OK
- Require explicit return and argument types on exported functions and class methods.
- Severity Warning | TS | 6 tests

### ts_init_declarations
**Status**: OK
- Variables should be initialized at declaration — uninitialized declarations are error-prone.
- Severity Warning | TS | 3 tests

### ts_max_params
**Status**: OK
- Functions with too many parameters are hard to understand and maintain.
- Severity Warning | TS | 4 tests

### ts_member_ordering
**Status**: OK
- Class and interface members should follow a consistent order: signatures, fields, constructors, m...
- Severity Warning | TS | 3 tests

### ts_method_signature_style
**Status**: OK
- Shorthand method signatures in interfaces are less safe than property signatures — they allow uns...
- Severity Warning | TS | 3 tests

### ts_no_array_constructor
**Status**: OK
- Generic `Array` constructor is ambiguous — use array literal notation `[]`.
- Severity Warning | TS | 4 tests

### ts_no_confusing_non_null_assertion
**Status**: OK
- `a! == b` looks confusingly like `a !== b`.
- Severity Warning | TS | 4 tests

### ts_no_const_enum
**Status**: OK
- `const enum` declarations are inlined and incompatible with isolatedModules.
- Severity Warning | TS | 4 tests

### ts_no_dupe_class_members
**Status**: OK
- Duplicate class members shadow earlier definitions and indicate a bug.
- Severity Error | TS | 3 tests

### ts_no_duplicate_enum_values
**Status**: OK
- Duplicate enum member values cause silent shadowing at runtime.
- Severity Warning | TS | 5 tests

### ts_no_dynamic_delete
**Status**: OK
- Using `delete` on a computed key is error-prone — use `Map` or `Set` instead.
- Severity Warning | TS | 5 tests

### ts_no_empty_function
**Status**: OK
- Empty functions are often a sign of incomplete refactoring.
- Severity Warning | TS | 4 tests

### ts_no_empty_object_type
**Status**: OK
- `{}` as a type matches any non-nullish value — it almost never means what you think.
- Severity Warning | TS | 4 tests

### ts_no_export_equal
**Status**: OK
- CommonJS-style `export = ...` — prefer ES module exports.
- Severity Warning | TS | 4 tests

### ts_no_extra_non_null_assertion
**Status**: OK
- Extra non-null assertions (`!!`) are redundant and confusing.
- Severity Warning | TS | 4 tests

### ts_no_extraneous_class
**Status**: OK
- Classes with only static members or an empty body should be plain objects or modules.
- Severity Warning | TS | 4 tests

### ts_no_implicit_any_catch
**Status**: OK
- catch binding without an explicit type annotation falls back to implicit any.
- Severity Warning | TS | 4 tests

### ts_no_import_type_side_effects
**Status**: OK
- Inline `type` qualifiers on every specifier leave a side-effect import at runtime.
- Severity Warning | TS | 3 tests

### ts_no_inferrable_types
**Status**: OK
- Explicit types on variables initialized with literals are redundant — TypeScript infers them.
- Severity Warning | TS | 5 tests

### ts_no_invalid_this
**Status**: OK
- `this` used outside a class or class-like object is likely a bug.
- Severity Warning | TS | 3 tests

### ts_no_invalid_void_type
**Status**: OK
- `void` is only valid as a return type or generic type argument.
- Severity Warning | TS | 4 tests

### ts_no_loop_func
**Status**: OK
- Functions declared inside loops often cause bugs due to closures capturing the loop variable by r...
- Severity Warning | TS/Rust | 6 tests

### ts_no_magic_numbers
**Status**: OK
- Magic numbers make code harder to understand — use named constants instead.
- Severity Warning | TS/Rust | 17 tests

### ts_no_misused_new
**Status**: OK
- Classes use `constructor()`, not `new()`. Interfaces use `new()`, not `constructor()`.
- Severity Warning | TS | 4 tests

### ts_no_mixed_types
**Status**: OK
- Interfaces and type aliases should not mix property signatures with method signatures.
- Severity Warning | TS | 5 tests

### ts_no_namespace
**Status**: OK
- TypeScript `namespace` is a legacy construct — use ES modules instead.
- Severity Warning | TS | 4 tests

### ts_no_non_null_asserted_nullish_coalescing
**Status**: OK
- `x! ?? y` is contradictory — `!` asserts non-null, `??` handles null.
- Severity Warning | TS | 3 tests

### ts_no_non_null_asserted_optional_chain
**Status**: OK
- Non-null assertion after optional chain contradicts its purpose.
- Severity Warning | TS | 4 tests

### ts_no_non_null_assertion
**Status**: OK
- Non-null assertions (`value!`) suppress compiler checks and can hide real nullability bugs.
- Severity Warning | TS | 5 tests

### ts_no_redeclare
**Status**: OK
- Redeclaring a variable in the same scope shadows the previous declaration silently.
- Severity Warning | TS | 3 tests

### ts_no_restricted_imports
**Status**: OK
- Disallow imports whose module specifier matches a configured pattern list.
- Severity Warning | TS | 5 tests

### ts_no_restricted_types
**Status**: OK
- Certain types are banned by project convention or because better alternatives exist.
- Severity Warning | TS | 4 tests

### ts_no_shadow
**Status**: OK
- Variable shadowing makes code harder to reason about and can lead to bugs.
- Severity Warning | TS | 4 tests

### ts_no_this_alias
**Status**: OK
- Assigning `this` to a variable is a legacy pattern — use arrow functions instead.
- Severity Warning | TS | 4 tests

### ts_no_unnecessary_parameter_property_assignment
**Status**: OK
- Assigning `this.x = x` in a constructor is redundant when `x` is already a parameter property.
- Severity Warning | TS | 3 tests

### ts_no_unnecessary_type_constraint
**Status**: OK
- `<T extends any>` and `<T extends unknown>` are unnecessary — all types already extend these.
- Severity Warning | TS | 3 tests

### ts_no_unsafe_declaration_merging
**Status**: OK
- Unsafe declaration merging between classes and interfaces.
- Severity Warning | TS | 4 tests

### ts_no_unused_expressions
**Status**: OK
- Expression statements that produce a value but discard it are likely mistakes.
- Severity Warning | TS | 5 tests

### ts_no_unused_private_class_members
**Status**: OK
- Private class members that are never used are dead code.
- Severity Warning | TS | 3 tests

### ts_no_unused_vars
**Status**: OK
- Declared variables that are never used are dead code.
- Severity Warning | TS | 5 tests

### ts_no_use_before_define
**Status**: OK
- Using variables before their definition leads to confusing code and potential TDZ errors.
- Severity Warning | TS | 3 tests

### ts_no_useless_constructor
**Status**: OK
- Empty constructors that only call `super()` are unnecessary.
- Severity Warning | TS | 4 tests

### ts_no_useless_empty_export
**Status**: OK
- `export {}` is unnecessary when the file already has other exports.
- Severity Warning | TS | 4 tests

### ts_no_wrapper_object_types
**Status**: OK
- Use lowercase primitives (`string`, `number`, `boolean`) instead of wrapper object types.
- Severity Warning | TS | 6 tests

### ts_only_throw_error
**Status**: OK
- Only `Error` instances should be thrown — primitives and plain objects lose stack traces.
- Severity Warning | TS | 7 tests

### ts_parameter_properties
**Status**: OK
- Parameter properties mix declaration and assignment — prefer explicit class properties.
- Severity Warning | TS | 3 tests

### ts_prefer_for_of
**Status**: OK
- A `for` loop whose index is only used for array access can be a simpler `for-of`.
- Severity Warning | TS | 3 tests

### ts_prefer_function_type
**Status**: OK
- An interface with only a call signature should be a function type.
- Severity Warning | TS | 3 tests

### ts_prefer_literal_enum_member
**Status**: OK
- Enum members should be initialized with literal values, not computed expressions.
- Severity Warning | TS | 6 tests

### ts_prefer_promise_reject_errors
**Status**: OK
- `Promise.reject()` should receive an `Error` instance, not a primitive or plain object.
- Severity Warning | TS | 8 tests

### ts_prefer_satisfies
**Status**: OK
- `as Type` on object/array literal widens the type — use `satisfies` instead.
- Severity Warning | TS | 5 tests

### ts_prefer_using_declaration
**Status**: OK
- try/finally with a single cleanup call is replaceable by `using` / `await using` (TS 5.2+).
- Severity Warning | TS | 5 tests

### ts_triple_slash_reference
**Status**: OK
- Triple-slash reference directives are legacy — use ES `import` instead.
- Severity Warning | TS | 4 tests

### ts_unified_signatures
**Status**: OK
- Function overload signatures that differ by a single parameter should be unified with a union or ...
- Severity Warning | TS | 3 tests

### use_type_alias
**Status**: OK
- Repeated complex inline type annotations should be extracted into a type alias.
- Severity Warning | TS | 4 tests

### useless_string_operation
**Status**: OK
- String method result is ignored \u{2014} strings are immutable.
- Severity Error | TS | 6 tests

### valid_describe_callback
**Status**: OK
- `describe` callback must be a synchronous function with no parameters and no return value.
- Severity Warning | TS | 10 tests

### vitest_hoisted_apis_on_top
**Status**: OK
- `vi.mock` / `vi.hoisted` are hoisted above imports — placing them after imports misleads readers.
- Severity Warning | TS | 5 tests

### vitest_no_disabled_tests
**Status**: OK
- Disabled tests (`xtest`, `xit`, `xdescribe`, `.skip`) silently erode coverage.
- Severity Warning | TS | 6 tests

### vue_define_emits_typed
**Status**: OK
-     description:
- Severity Warning | Text | 2 tests

### vue_markraw_for_third_party
**Status**: OK
- Wrap third-party instances (Chart.js, maps, editors, ...) in `markRaw()`.
- Severity Warning | Text | 8 tests

### vue_no_duplicate_v_if
**Status**: OK
- Two opposite `v-if` conditions should be `v-if`/`v-else`.
- Severity Warning | Text | 3 tests

### vue_no_mutate_prop
**Status**: OK
- Don't mutate a prop directly — props are one-way.
- Severity Warning | Text | 8 tests

### vue_no_options_api
**Status**: OK
- Use Composition API (`<script setup>`), not Options API.
- Severity Error | Text | 3 tests

### vue_no_reactive_destructure
**Status**: OK
- Destructuring `reactive()` breaks reactivity — use `toRefs()` or `ref()`.
- Severity Error | Text | 5 tests

### vue_pinia_store_to_refs
**Status**: OK
- Destructuring a Pinia store without `storeToRefs()` loses reactivity.
- Severity Warning | Text | 3 tests

### vue_prefer_computed
**Status**: OK
- Use `computed()` when a watcher only assigns a derived value to another ref.
- Severity Warning | Text | 7 tests

### vue_prefer_v_else
**Status**: OK
- Consecutive `v-if=\
- Severity Warning | Text | 2 tests

### vue_require_lifecycle_cleanup
**Status**: OK
-     description:
- Severity Warning | Text | 2 tests

### vue_script_setup_required
**Status**: OK
- `<script>` without `setup` attribute uses Options-API-style Composition API — use `<script setup>...
- Severity Warning | Text | 2 tests

### vue_sfc_section_order
**Status**: OK
- SFC sections must be ordered: `<script setup>` → `<template>` → `<style>`.
- Severity Warning | Text | 2 tests

### vue_url_state_for_filters
**Status**: OK
- Store filter/pagination state in the URL, not in local `ref()`.
- Severity Warning | Text | 9 tests

### vue_v_for_needs_stable_key
**Status**: OK
- v-for `:key` must use a stable identifier, not the loop index.
- Severity Error | Text | 4 tests

### xpath_injection
**Status**: OK
- Detects potential XPath injection via dynamic query strings.
- Severity Error | TS | 5 tests

### xstate_entry_exit_action
**Status**: OK
- `entry` and `exit` must be a string, a function, or an array of those.
- Severity Warning | TS | 6 tests

### xstate_event_names
**Status**: OK
- XState event names must be SCREAMING_SNAKE_CASE.
- Severity Warning | TS | 5 tests

### xstate_invoke_usage
**Status**: OK
- `invoke` must be an object (or array of objects) with at least a `src` property.
- Severity Warning | TS | 6 tests

### xstate_no_async_guard
**Status**: OK
- XState `guard`/`cond` properties must be synchronous — async functions are not supported.
- Severity Error | TS | 7 tests

### xstate_no_imperative_action
**Status**: OK
- `send()` / `raise()` must only be called inside an action context.
- Severity Warning | TS | 6 tests

### xstate_no_infinite_loop
**Status**: OK
- XState `always` transitions without a guard that stay in (or re-target) the same state cause infi...
- Severity Error | TS | 6 tests

### xstate_no_inline_implementation
**Status**: OK
- Inline functions as XState `actions`, `guards`, or `services` hinder reuse and testing.
- Severity Warning | TS | 8 tests

### xstate_no_invalid_conditional_action
**Status**: OK
- XState `choose(...)` branches must declare both a `guard`/`cond` and `actions` property.
- Severity Warning | TS | 7 tests

### xstate_no_invalid_state_props
**Status**: OK
- Unknown property on an XState state node — likely a typo or misplaced config.
- Severity Warning | TS | 6 tests

### xstate_no_invalid_transition_props
**Status**: OK
- Transition objects in XState `on` handlers must only use known properties.
- Severity Warning | TS | 6 tests

### xstate_no_misplaced_on_transition
**Status**: OK
- XState `on` must live on state nodes, not inside `invoke` or directly under `states`.
- Severity Warning | TS | 6 tests

### xstate_no_ondone_outside_compound_state
**Status**: OK
- XState `onDone` is only valid on compound states (with nested `states`) or invoking states (with ...
- Severity Warning | TS | 6 tests

### xstate_spawn_usage
**Status**: OK
- `spawn()` must be called inside an `assign()` action.
- Severity Warning | TS | 5 tests

### xstate_state_names
**Status**: OK
- State names inside `states: { ... }` must be camelCase or snake_case.
- Severity Warning | TS | 5 tests

### zod_brand_ids
**Status**: OK
- ID-like fields (`id`, `userId`, `post_id`) benefit from \
- Severity Warning | TS | 8 tests

### zod_consistent_import_source
**Status**: OK
- Imports from non-standard zod subpaths (e.g., `zod/v4`, `zod/mini`) cause \
- Severity Warning | TS | 4 tests

### zod_no_any
**Status**: OK
- `z.any()` disables validation and type narrowing.
- Severity Warning | TS | 2 tests

### zod_no_empty_custom_schema
**Status**: OK
- `z.custom()` without a validator function accepts any value.
- Severity Warning | TS | 5 tests

### zod_no_number_schema_with_int
**Status**: OK
- Use `z.int()` instead of `z.number().int()` in Zod v4+.
- Severity Warning | TS | 3 tests

### zod_no_optional_and_default_together
**Status**: OK
- Chaining `.optional()` and `.default()` on the same schema is redundant.
- Severity Warning | TS | 5 tests

### zod_no_optional_nullable_chain
**Status**: OK
- `.optional().nullable()` should be written as `.nullish()` for clarity.
- Severity Warning | Text | 3 tests

### zod_no_string_schema_with_uuid
**Status**: OK
- `z.string().uuid()` is deprecated in Zod v4 — use the top-level `z.uuid()` schema.
- Severity Warning | TS | 5 tests

### zod_no_throw_in_refine
**Status**: OK
- `throw` inside `.refine()` / `.superRefine()` bypasses Zod's issue aggregation and surfaces as an...
- Severity Warning | TS | 6 tests

### zod_no_transform_in_record_key
**Status**: OK
- `.transform()` inside a `z.record()` key schema mutates the object key after validation, causing ...
- Severity Warning | TS | 5 tests

### zod_no_unknown_schema
**Status**: OK
- `z.unknown()` accepts anything — the schema provides no validation.
- Severity Warning | TS | 3 tests

### zod_prefer_discriminated_union
**Status**: OK
- `z.union([z.object({...}), ...])` with shared discriminant fields should use `z.discriminatedUnio...
- Severity Warning | Text | 2 tests

### zod_prefer_enum_over_literal_union
**Status**: OK
- `z.union([z.literal('a'), z.literal('b')])` with only string literals should use `z.enum([...])`.
- Severity Warning | TS | 8 tests

### zod_prefer_safe_parse
**Status**: OK
- `.parse()` in a route handler throws `ZodError` unhandled — use `.safeParse()` instead.
- Severity Warning | Text | 4 tests

### zod_prefer_top_level_format
**Status**: OK
- Zod v4 top-level format helpers are shorter and faster.
- Severity Warning | TS | 5 tests

### zod_refine_requires_path
**Status**: OK
- `z.object().refine()` without `path:` attaches the error to the whole object, not a specific field.
- Severity Warning | Text | 2 tests

### zod_require_error_messages
**Status**: OK
- `.refine()` without an error message produces unhelpful validation errors.
- Severity Warning | Text | 2 tests

### zod_require_schema_suffix
**Status**: OK
- Exported Zod schemas should be named with a `Schema` suffix.
- Severity Warning | TS | 5 tests

### zod_string_min_1_required
**Status**: OK
- Bare `z.string()` without length constraints accepts empty strings.
- Severity Warning | Text | 4 tests

### zod_transform_requires_pipe
**Status**: OK
- `.transform()` returns an untyped value — follow with `.pipe(z.*)` to re-validate.
- Severity Warning | TS | 5 tests

### zod_trim_before_min
**Status**: OK
- `z.string().min(1)` without `.trim()` allows strings of only whitespace.
- Severity Warning | Text | 2 tests

### zod_validate_env_at_startup

**Status**: OK
- `process.env.X` is read without an accompanying Zod \
- Severity Warning | TS | 6 tests
---

## Résumé

**Règles auditées**: 981 règles (5871 tests passent)

**Issues trouvées**:

| Règle | Sévérité | Description |
|-------|----------|-------------|
| `no_typeof_undefined` | **ISSUE** | Le conseil "Use `x === undefined`" peut causer ReferenceError si `x` n'est pas déclaré. `typeof` est le seul moyen safe de vérifier une variable potentiellement non déclarée. |
| `no_duplicate_imports` | MINOR | Utilise TextCheck ligne par ligne — ne gère pas les imports multi-lignes |
| `no_redundant_boolean` | MINOR | Pattern hybride text/AST dans `ast_check!`, risque de FP sur patterns dans des strings |

**Règles vérifiées par catégorie**:
- **Accessibilité (a11y)**: 33 règles — patterns JSX cohérents
- **React/JSX**: 70 règles (react_jsx_key, react_no_*, react_prefer_*, react_server_action_*, etc.)
- **TypeScript (ts_)**: 67 règles — validation types, classes, génériques
- **Rust**: 52 règles (rust_no_unwrap, rust_no_panic_macros, rust_prefer_*, rust_sync_io_in_async, etc.)
- **Regex**: 52 règles — détection patterns problématiques, optimisations
- **Vue**: 16 règles (vue_no_v_html_unsafe, vue_no_mutate_prop, vue_prefer_computed, etc.)
- **Playwright**: 35 règles — bonnes pratiques tests E2E
- **JSDoc**: 27 règles — documentation, types, tags
- **Zod**: 22 règles — validation schemas, bonnes pratiques
- **Tanstack (Query/Start)**: 20 règles — React Query, TanStack Start
- **HTML**: 19 règles — accessibilité, sémantique, sécurité
- **Tailwind**: 15 règles — classes, ordre, conflits
- **XState**: 14 règles — machines d'état, transitions
- **Node.js**: 14 règles — callbacks, paths, process
- **i18n**: 13 règles — traductions, placeholders, ICU
- **SQL**: 13 règles — injection, performance, types
- **require_***: 11 règles — validations obligatoires
- **Import**: 9 règles — cycles, doublons, conventions
- **Drizzle ORM**: 8 règles — migrations, schemas, sécurité
- **Hono**: 8 règles — cookies, CSRF, headers sécurité
- **Better Auth**: 7 règles — CSRF, cookies, rate limiting
- **FSD (Feature-Sliced Design)**: 4 règles — architecture
- **prefer_* (modernisation)**: 87 règles — APIs modernes, syntaxe ES2020+
- **no_* (interdictions)**: 256 règles — anti-patterns, sécurité, bugs
- **Cross-file analysis**: no_identical_functions, inconsistent_function_call, dead_export, arguments_order, data_clumps

**Observations générales**:
- La grande majorité des règles sont bien implémentées avec des tests cohérents
- Bonne utilisation des helpers partagés (test_helpers, walker, sql_helpers, jsx, rust_helpers, vue_template_helpers)
- Bonnes heuristiques pour éviter les faux positifs (seuils, contextes, exclusions)
- Documentation inline (//! docblocks) généralement présente et utile
- Cross-file analysis bien implémentée via ImportIndex avec cache process-wide
- Règles de sécurité solides avec détection multi-pattern
- Support multi-langage cohérent (TS/JS/TSX/Rust/Vue/JSON)

**Patterns d'implémentation observés**:
- `ast_check!` macro pour règles AST simples
- `AstCheck` trait avec `walk_tree` pour règles complexes nécessitant tree walking
- `TextCheck` trait pour règles text-only (comments, SQL, secrets, i18n JSON)
- Utilisation de `ImportIndex` pour analyse cross-file (exports, imports, usages, call_sites)
- Seuils configurables via `ctx.config.threshold()`
- Tests avec tempfile pour règles cross-file
- Backends multiples par règle (typescript.rs, rust.rs, vue.rs, text.rs)
- Helpers partagés: `is_in_test_context()`, `jsx_attribute_name()`, `call_function_name()`, etc.

