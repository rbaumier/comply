# Règles à investiguer / fixer

Liste brute extraite des `@TODO` que j'ai laissés dans `src/rules/mod.rs`
après avoir lancé `comply` sur le projet. Chaque entrée = une observation,
**à investiguer avant toute décision** (fix de la règle, suppression,
exclusion, ou faux signal de ma part).

On les prend dans l'ordre. Pour chaque entrée :
1. Vérifier si la règle est valide ou cassée.
2. Décider : **Fix** / **Supprimer** / **Restreindre langage** / **Faux positif acceptable**.
3. Mettre à jour la `Décision` ci-dessous.

---

## 1. `max-function-lines` — configurabilité + version Rust ✅

**Source :** `mod.rs:838`
**Observation :**
```
src/rules/no_useless_error_capture_stack_trace/typescript.rs:28:5:
warning [max-function-lines] this function has too many lines (124/120)
```

**Investigation :**
- La règle était **déjà configurable** (`defaults.rs:21` → `max = 30`). Le `124/120` du log ne venait PAS de la règle native TS — elle était déléguée pour Rust à `clippy::too_many_lines` via `register_ts_family_with_clippy_marker!`, et clippy utilisait son propre seuil (120) indépendamment.
- Deuxième problème révélé : TS comptait les **lignes physiques** (y compris blanches et commentaires), clippy comptait les **lignes logiques**. Deux backends, deux sémantiques.

**Décision : fix (backend Rust natif + NCLOC partout).**
- Remplacer le marker clippy par un backend Rust tree-sitter natif (`src/rules/max_function_lines/rust.rs`).
- Nouvelle métrique : **NCLOC** (Non-Commented Lines Of Code) — lignes physiques moins blanches moins commentaires pur. Helper partagé `count_ncloc` dans `mod.rs`, scanner ligne par ligne avec tracking des `/* … */` depuis le début du fichier.
- Un seul seuil global (`max = 30`), pas d'override par langage.
- Kinds flaggés en Rust : `function_item` (incluant `impl`, `async`, trait defaults) et `closure_expression`.
- Tests partagés dans `shared_tests.rs` qui cross-check les deux backends sur les mêmes scénarios.

**Résultat :**
- 23 tests verts pour la règle (dont 9 unit tests pour `count_ncloc` et 1 test de cohérence cross-backend sur 6 scénarios).
- Le faux positif initial à `no_useless_error_capture_stack_trace/typescript.rs:28` est maintenant flaggé à **126 NCLOC** (cohérent avec la sémantique NCLOC), comme une vraie violation.
- Le diagnostic sur `all_rule_defs` (src/rules/mod.rs:832) tombe à **667 NCLOC** — c'est une table de données plate, à gérer séparément (override dans `comply.toml` ou split en helpers).
- 3473 tests passent au total, aucune régression.

---

## 2. `todo-needs-issue-link` — flag du commentaire descriptif d'une autre règle ✅

**Décision : règle supprimée.** Fait dans le commit de refactor (`1be9f36d refactor: prune issue-link rules, …`). Plus de détection « TODO/BUG sans issue link » dans comply.

---

## 3. `no-commented-out-code` — flag des commentaires de syntaxe ✅

**Décision : réécriture complète avec mini-parsing tree-sitter.** Détails de l'approche, des limitations et des faux-négatifs acceptés : voir le docblock de `src/rules/no_commented_out_code/mod.rs`.

---

## 4. `exports-at-top` — désactivée pour Rust ? ✅

**Décision : règle supprimée entièrement (TS + Rust).** Le principe « regrouper par logique, pas par visibilité » vaut aussi bien pour TypeScript que pour Rust. Forcer tous les exports en tête casse le flux naturel d'un fichier (type exporté → helpers privés → fonction publique utilisant les deux). `src/rules/exports_at_top/` supprimé, registration et module declaration retirés de `mod.rs`.

---

## 5. `no-type-encoded-names` — flag `fn_name` ✅

**Décision : nettoyage du set `TYPE_PREFIXES` (faux amis retirés, legacy ajoutés).** Détails dans le docblock de `src/rules/no_type_encoded_names/type_prefix.rs`.

---

## 6. `no-multi-op-oneliner` — flag un `assert_eq!` de test ✅

**Décision : ignorer les ranges de comment nodes (tree-sitter) avant compter les opérateurs.** Le scanner naïf comptait les `/`, `-`, `.` dans le trailing `// comment` (4 ops réels + 7 noise = 11 reportés). Détails dans le docblock de `src/rules/no_multi_op_oneliner/dense_lines.rs`.

---

## 7. `rust-explicit-iter-loop` — flag `for &b in bytes.iter()` ⏸

**Décision : pas un bug — clippy a raison, le code est sub-optimal.** La règle est une pure délégation à `clippy::explicit_iter_loop`. En Rust 2021+, `&[T]: IntoIterator<Item = &T>`, donc `for &b in bytes.iter()` est strictement équivalent à `for &b in bytes` en plus verbeux. Le binding par valeur via `&b` est préservé dans les deux cas.

**Action déférée** : nettoyer toutes les occurrences `for X in Y.iter() {` de la codebase en un batch séparé (pas un fix de règle).

---

## 8. `sql-no-between-timestamp` — flag les `BETWEEN` dans les commentaires ✅

**Décision : réécriture en AstCheck ciblant les string literals SQL.** TextCheck → AstCheck (TS + Rust + Vue), détection SQL via helper partagé `sql_helpers::is_sql_string` (DML keyword + `WHERE`/`FROM`, whole-word matching). Détails dans le docblock de `src/rules/sql_no_between_timestamp/mod.rs`. Helper `walker::collect_nodes_of_kinds` promu pour réutilisation cross-rule. Règle re-activée dans le registry.

---

## 9. `sql-no-offset-pagination` — flag le mot `offset` dans une liste ✅

**Décision : réécriture en AstCheck ciblant les string literals SQL** (même infra que #8). Détails dans le docblock de `src/rules/sql_no_offset_pagination/mod.rs`. Règle re-activée.

---

## 10. `sql-no-varchar` — flag dans un test de regex ✅

**Décision : réécriture en AstCheck ciblant les string literals DDL.** Nouveau filtre `sql_helpers::is_sql_ddl` (CREATE/ALTER + TABLE/TYPE) + `word_followed_by_open_paren` pour matcher `VARCHAR(` / `CHAR(` au mot près. Détails dans le docblock de `src/rules/sql_no_varchar/mod.rs`. Règle re-activée.

---

## 11. `db-no-string-concat-sql` — flag des `format!()` qui ne sont pas SQL ✅

**Décision : isoler le scope de détection au format string seul** (Rust : 1ʳᵉ string literal du token_tree ; TS : seules les sides string du `binary_expression`), puis filtrer via `is_sql_string` (whole-word). Le scan du texte complet de la macro voyait `String::from_utf8_lossy` → `FROM` substring. Détails dans le docblock de `src/rules/db_no_string_concat_sql/mod.rs`. Règle re-activée.

---

## 12. `no-empty-collection-use` — flag `HashMap::new()` ✅

**Décision : règle supprimée.** Le pattern « empty collection → never written → read » demande une vraie analyse de flux intra-procédurale. Ni clippy, ni oxlint, ni eslint ne l'ont : ils délèguent au compilateur (`unused_mut`, `dead_code`, `noUnusedLocals`). L'implémentation précédente était une heuristique lexicale à window de 5 lignes qui produisait le FP user sur un pattern idiomatique. Règle supprimée entièrement (`src/rules/no_empty_collection_use/`).

---

## 13. `prefer-immediate-return` — flag un parser muté avant return ✅

**Décision : réécrit en vrai AstCheck.** L'ancienne version était text-based (paires de lignes non-blanches lexicales) et matchait `parser` comme tail expression alors que c'était le début d'une méthode chain formatée sur plusieurs lignes. Le nouveau walker itère les *named children* consécutifs des `block` (Rust) / `statement_block` (TS) et match exactement `(let_declaration, return_expression/tail(identifier=X))`. Détails dans le docblock de `src/rules/prefer_immediate_return/rust.rs`. Règle re-activée.

---

## 14. `no-clear-text-protocol` — flag `http://` dans les commentaires ✅

**Décision : réécriture en AstCheck sur string literals + filtre length-strictly-greater-than-prefix.** Élimine les FP de commentaires (jamais visités par l'AST walk) et les FP de prefixes nus (`"http://".len() == 7` → ne flag pas, c'est une needle de détection). Détails dans le docblock de `src/rules/no_clear_text_protocol/mod.rs`.

---

## 15. `no-duplicate-string` — flag des fragments de schéma JSON commenté ✅

**Décision : réécriture en AstCheck sur les string literals.** Les raw strings Rust (`r#"{…}"#`) sont vues comme un seul node ; leur contenu n'est pas ré-analysé quote par quote. Les commentaires ne sont jamais visités. Détails dans le docblock de `src/rules/no_duplicate_string/mod.rs`. Règle re-activée.

---

## 16. `no-ignored-exceptions` — flag `let _ = …` dans un `#[test]` ✅

**Décision : skip dans le test context.** La règle Rust flagge `let _ = fallible()` parce que `let _ =` discard explicite un Result. Mais dans un `#[test]`, c'est l'idiome pour « call and don't care, juste vérifier qu'il n'y a pas de panic ». Le helper partagé `rust_helpers::is_in_test_context` (déjà utilisé par `rust_no_unwrap` et `rust_no_panic_macros`, dont les copies locales sont retirées au passage) est appelé en début de check pour skip les `#[test]` fns et `#[cfg(test)]` mods.

---

## 17. `no-nested-switch` — flag les `match` Rust imbriqués ✅

**Décision : drop le backend Rust, garder TS/JSX uniquement.** Rust `match` est l'idiome de dispatch (exhaustif, pas de fall-through, scope par arm) et un `match` imbriqué dans une arm est une refinement hiérarchique normale. Les cas où une vraie explosion de complexité justifierait une extraction sont déjà couverts par `cognitive-complexity` et `cyclomatic-complexity` (qui ont tous deux des backends Rust). Détails dans le docblock de `src/rules/no_nested_switch/mod.rs`.

---

## 18. `function-inside-loop` — règle supprimée entièrement ✅

**Décision : règle supprimée (TS + Rust).** En Rust, les closures compilent vers des structs anonymes stack-allouées à coût zéro ; le FP user (`|r| r.id == file_id` passé à `find()` avec capture d'une var loop-scoped) est l'idiome pur de prédicat sur combinator et ne peut pas être déplacé hors du loop. Côté JS/TS, le rationale de la règle s'est largement érodé avec les JIT modernes, et eslint couvre déjà le sous-cas réellement bogué (`no-loop-func` sur capture de var mutée). Pas assez de valeur pour justifier le maintien. `src/rules/function_inside_loop/` supprimé, registration retirée de `mod.rs`.

---

## 19. `nested-control-flow` — compte les `impl` comme niveau d'imbrication ✅

**Décision : collapse des `else if` cascades + reset de depth à la frontière de fonction/closure, aligné sur eslint `max-depth`.** La vraie cause du FP n'était pas `impl` (jamais compté) mais la cascade `else if` : tree-sitter-{rust,typescript} parse `else if` comme `if_* → else_clause → if_*`, gonflant la profondeur artificiellement. Le backend collapse ces cascades (même logique qu'eslint `if (node.parent.type !== "IfStatement")`) et reset la profondeur à chaque `function_item`/`closure_expression` en Rust et chaque callable en TS, comme le `functionStack` d'eslint. MAX_DEPTH reste à 3. Détails dans le docblock de `src/rules/nested_control_flow/mod.rs`. 21 tests verts (rust + typescript + shared_tests). Les 4 FPs sur `src/files.rs` disparaissent.

---

## 20. `no-duplicated-branches` — heuristique fausse + diagnostic dupliqué ✅

**Décision : pattern-binding mode + dedup, plus fix d'un bug latent sur les match arms.** Quand une chaîne `if/else if` Rust contient au moins une `let_condition` (`if let PAT = EXPR`), la clé de comparaison passe de `body` seul à `(condition, body)` — deux `if let` peuvent partager un body textuellement identique référençant un binding `r` qui est en réalité distinct dans chaque branche. Si **les deux** sont identiques (condition ET body), c'est encore flaggé (vrai duplicate). Côté dedup, chaque ligne dup est rapportée au plus une fois (les anciens loops O(n²) émettaient `j` une fois par match antérieur, donc 3 diagnostics pour 3 branches identiques au lieu de 2). Bug latent corrigé : la version précédente itérait les children de `match_expression` au lieu de descendre dans `match_block` — aucun match arm n'était en réalité comparé. Détails dans le docblock de `src/rules/no_duplicated_branches/mod.rs`. 16 tests verts, 0 FP sur le fichier où le bug a été initialement observé.

---

## 21. `no-os-command` — parfois inévitable ✅

**Décision : règle supprimée (TS + Rust).** L'implémentation flaggait littéralement tous les `Command::new` / `exec` / `spawn` sans aucune analyse de dataflow — l'équivalent d'une règle « no-fs-write » qui flaggerait tous les `fs::write`. Comply lui-même a 13+ usages légitimes (orchestrator git/oxlint/clippy/knip/jscpd/llm-cli). Sans mécanisme « security hotspot / review » côté comply, et sans taint analysis, c'est pure friction. Le backend TS avait en plus un bug latent : il scannait les string literals contenant `"child_process"` et flaggerait des commentaires de doc. **Réorienté vers la liste LLM** : la détection d'injection de commande demande de raisonner sur l'origine des arguments, ce que seul un LLM peut faire en l'absence de dataflow. Ajouté à `project_comply_next_steps.md`. `src/rules/no_os_command/` supprimé.

---

## 22. `no-redundant-jump` — flag un return non redondant ✅

**Décision : réécriture complète en vrai AstCheck.** L'ancienne impl était un scanner ligne-par-ligne déguisé : trouver `return;`, next non-blank line doit être `}`, next non-blank line après ce `}` doit être EOF ou un autre `}`. Ce heuristique traitait deux `}` consécutifs comme « fin de scope » même quand le scope *extérieur* avait encore du code. Le nouvel algo walk up depuis le nœud jusqu'à une frontière de fonction (pour `return;`) ou de loop (pour `continue;`), exigeant une position de queue à chaque `block`/`statement_block` (dernier `named_child`) et traitant `if`/`else`/`match`/`switch_case` comme des wrappers transparents (branches parallèles = tails parallèles). `return;` avec valeur et `continue label;` skippés (seules les formes nues sont considérées). Détails dans le docblock de `src/rules/no_redundant_jump/mod.rs`. 23 tests verts, 0 FP sur le fichier initial. Règle re-activée.

---

## 23. `no-sql-string-format` — flag un `format!()` qui n'est pas SQL ✅

**Décision : règle supprimée entièrement — doublon (strictement inférieur) de `db-no-string-concat-sql`.** Le backend Rust matchait n'importe quel substring uppercased (`SELECT`/`INSERT`/`UPDATE`/`DELETE`/`WHERE`), donc le prompt LLM de `build_prompt` dans `src/llm/unified_prompt.rs` — qui contient de la prose anglaise avec « delete this », « updateRoleField » etc. — était flaggé. `db-no-string-concat-sql` (résolu en #11) couvre déjà les mêmes macros Rust avec le helper robuste `is_sql_string` (whole-word + DML keyword + WHERE/FROM confirmation). Le backend TS était un scanner ligne-par-ligne équivalent, tout aussi fragile. Gap restant sur les template literals TS (ex: `` `SELECT ... ${id}` ``) adressé dans l'entrée suivante en étendant `db-no-string-concat-sql`. `src/rules/no_sql_string_format/` supprimé.

---

## 24. `no-weak-cipher` — règle qui flag son propre `id`/`doc_url` ✅

**Décision : réécriture complète des deux backends, alignée sur SonarJS `S5547`.** L'ancienne impl Rust scannait tous les string literals avec un `contains("-des")` qui matchait le `-des` prefix de `-description`. Le nouvel algo est **narrow par contexte, loose par contenu** : TS walk `call_expression` dont le callee trailing name est `createCipheriv` et check si le 1er arg string literal commence par l'un des 5 prefixes `bf`/`blowfish`/`des`/`rc2`/`rc4` ; Rust walk `call_expression` dont le function est un `scoped_identifier` de la forme `[<path>::]Cipher::<weak_name>` où `<weak_name>` commence par un de ces prefixes suivi de `_` ou end-of-identifier. Le backend Rust ne regarde plus du tout les string literals — c'était structurellement faux parce que les crypto crates Rust (`openssl::symm::Cipher::des_ecb()`, etc.) dispatchent le cipher par **nom de méthode**, pas par string. Pas de const-propagation v1 (gap connu, à ajouter plus tard). Détails dans le docblock de `src/rules/no_weak_cipher/mod.rs`. 20 tests verts, 0 FP sur le fichier initial. Règle re-activée.

---

## 25. `no-loop-counter-reassign` — flag un `while` valide ✅

**Décision : drop le backend Rust, garder TS/JSX.** Même traitement que #17. Le concept « loop counter reassign » n'a pas d'équivalent en Rust : `for x in iter` est une pattern binding immutable (réassignation rejetée par le compiler), et `while`/`loop` n'ont pas de counter — l'avancement variable de `i` via `i = end` / `i += 1` est l'idiome normal d'une boucle while-avec-state. Le backend TS cible le vrai anti-pattern (C-style `for (let i=0; i<n; i++) { i = 5; }` qui casse le contrat d'itération comptée) et est conservé tel quel. Détails dans le docblock de `src/rules/no_loop_counter_reassign/mod.rs`. 4 tests TS verts, 0 FP sur le fichier initial.

---

## 26. `no-misplaced-loop-counter` — flag un `while` avec `count` et `p` distincts ✅

**Décision : drop le backend Rust, garder TS/JSX, re-activer la règle.** Même traitement que #17 / #25. Le backend Rust extrayait le premier identifier du condition texte et matchait le **premier** ` += 1` du body — donc sur `while p > 0 && bytes[p - 1] == b'\\' { count += 1; p -= 1; }`, il croyait que `count` était « le update » et flaggait le mismatch avec `p`. Mais `while`/`loop` Rust n'ont pas de clause d'update séparée, et les boucles à état composé (count + p pour scanner les `\` en arrière, lo/hi/mid en binary search, two-pointers) mutent légitimement plusieurs variables. Le backend TS cible le vrai bug (`for (let i = 0; i < n; j++)` — typique copy-paste) via les fields tree-sitter propres (`condition`/`increment`) et est conservé tel quel. Détails dans le docblock de `src/rules/no_misplaced_loop_counter/mod.rs`. 4 tests TS verts, 0 FP sur `src/rules/consistent_template_literal_escape/typescript.rs`. Règle re-activée.

---

## 27. `no-timing-attack` — flag une comparaison de strings tree-sitter ✅

**Décision :** Déjà fixée. La règle a été réécrite en AstCheck et ne flag plus les string literals comme `"index_signature"`. Test de régression en place.

---

## 28. `no-non-literal-fs-filename` — toujours pertinent ? ✅

**Décision :** Règle jamais implémentée (pas de dossier `src/rules/no_non_literal_fs_filename/`).

---

## 29. `blank-line-between-blocks` — TODO_AFTER_REVIEW ✅

**Décision :** Règle jamais implémentée (pas de dossier).

---

## 30. `intermediate-variables` — flag du code peu imbriqué ✅

**Décision :** Le fichier flaggé (`no_skipped_test_without_link/rust.rs`) n'existe plus. Pas de FP actuel sur la codebase. Règle OK.

---

## 31. `justify-inaction` — TODO_AFTER_REVIEW ✅

**Décision :** Règle active, pas de FP actuel sur la codebase.

---

## 32. `no-hidden-control-flow` — flag 2 conditions avec `&&` ✅

**Décision :** Règle jamais implémentée (pas de dossier).

---

## 33. `consistent-assert` — flag dans le code de test ✅

**Décision :** Règle jamais implémentée (pas de dossier).

---

## 34. `catch-error-name` — TODO_AFTER_REVIEW ✅

**Décision :** Règle active (unicorn), pas de FP actuel sur la codebase.

---

## 35. `no-zero-fractions` — flag `1.0` dans un calcul `f64::EPSILON` ✅

**Décision :** Backend Rust supprimé. En Rust, `1.0` est idiomatique et requis pour le typage f64. Règle réactivée pour TS/JS seulement.

---

## 36. `prefer-simple-condition-first` — flag des comparaisons légitimes ✅

**Décision :** Règle supprimée. Trop de FP sur des comparaisons équivalentes en complexité (casts, chaînes de `||`). Valeur ajoutée insuffisante vs bruit.

---

## 37. `comment-prose-quality` — flag de la rustdoc valide ✅

**Décision :** Fixée. La fonction `comment_text()` strip maintenant les marqueurs de doc-comment Rust (`//!`, `///`) avant analyse. Tests de régression ajoutés. Règle réactivée.
