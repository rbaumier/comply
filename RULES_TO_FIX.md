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

## 20. `no-duplicated-branches` — heuristique fausse + diagnostic dupliqué

**Source :** `mod.rs:1218`
**Observation :** flag des branches qui ne diffèrent que par un literal
différent :
```rust
let rest = if let Some(r) = trimmed.strip_prefix("let ") {
    r.trim_start()
} else if let Some(r) = trimmed.strip_prefix("const ") {
    r.trim_start()
} else if let Some(r) = trimmed.strip_prefix("var ") {
    r.trim_start()
}
```
**Et** l'erreur est reportée plusieurs fois sur les mêmes lignes :
```
src/rules/no_redundant_assignment/typescript.rs:32:1: warning [no-duplicated-branches] …
src/rules/no_redundant_assignment/typescript.rs:34:1: warning [no-duplicated-branches] …
src/rules/no_redundant_assignment/typescript.rs:34:1: warning [no-duplicated-branches] …
```

**Décision :** _à compléter_

---

## 21. `no-os-command` — parfois inévitable

**Source :** `mod.rs:1290`
**Observation :** « on est parfois obligé non ? » Règle commentée pour
l'instant.

**Décision :** _à compléter_

---

## 22. `no-redundant-jump` — flag un return non redondant

**Source :** `mod.rs:1295`
**Observation :**
```
src/rules/react_no_object_type_as_default_prop/typescript.rs:102:1:
warning [no-redundant-jump] Redundant `return;` — execution already falls through
```
Code flaggé :
```rust
if is_arrow {
    let Some(parent) = node.parent() else { return };
    if parent.kind() != "variable_declarator" { return; }
    let Some(name) = parent.child_by_field_name("name") else { return };
    let Ok(t) = name.utf8_text(source) else { return };
    if !t.starts_with(|c: char| c.is_ascii_uppercase()) {
        return;
    }
}

// Find the formal_parameters → object_pattern with defaults.
let mut stack = vec![node];
while let Some(current) = stack.pop() {
```
Le `return;` n'est pas redondant : il sort tôt avant la suite. Règle
commentée pour l'instant.

**Décision :** _à compléter_

---

## 23. `no-sql-string-format` — flag un `format!()` qui n'est pas SQL

**Source :** `mod.rs:1316`
**Observation :** prend des `format!` qui ne sont pas des requêtes SQL :
```rust
pub fn build_prompt(source: &str) -> String {
    format!(
        r#"You are a code quality auditor. Analyze the following source file…
Source file:
```
{source}
```"#
    )
}
```
Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 24. `no-weak-cipher` — règle qui flag son propre `id`/`doc_url`

**Source :** `mod.rs:1336`
**Observation :** flagged sur ces lignes :
```rust
id: "jsdoc-require-throws-description",
// et
doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-throws-description.md"),
```
Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 25. `no-loop-counter-reassign` — flag un `while` valide

**Source :** `mod.rs:1363`
**Observation :** flagged alors que c'est un `while` :
```rust
while i + 2 < len {
    if bytes[i] == b'/' && bytes[i + 1] == b'*' && bytes[i + 2] == b'*' {
        let start = i;
        // …
        if let Some(end_rel) = source[i + 3..].find("*/") {
            let end = i + 3 + end_rel + 2;
            blocks.push((start_line, &source[start..end]));
            i = end;
        } else {
            break;
        }
    } else {
        i += 1;
    }
}
```

**Décision :** _à compléter_

---

## 26. `no-misplaced-loop-counter` — flag un `while` avec `count` et `p` distincts

**Source :** `mod.rs:1381`
**Observation :**
```
src/rules/consistent_template_literal_escape/typescript.rs:102:5:
error [no-misplaced-loop-counter] Condition uses `p` but update modifies `count`
```
Code flaggé :
```rust
while p > 0 && bytes[p - 1] == b'\\' {
    count += 1;
    p -= 1;
}
```
Les deux variables sont mutées, c'est un compteur de slashes en parallèle.
Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 27. `no-timing-attack` — flag une comparaison de strings tree-sitter

**Source :** `mod.rs:1449`
**Observation :**
```
src/rules/ts_consistent_indexed_object_style/typescript.rs:27:1:
error [no-timing-attack] Direct comparison of a security-sensitive value
```
Code flaggé :
```rust
let member = named_children[0];
if member.kind() != "index_signature" {
    return;
}
```

**Décision :** _à compléter_

---

## 28. `no-non-literal-fs-filename` — toujours pertinent ?

**Source :** `mod.rs:1457`
**Observation :** « impossible de passer que des string litteral en file
name non ? » Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 29. `blank-line-between-blocks` — TODO_AFTER_REVIEW

**Source :** `mod.rs:1474`
**Observation :** marquée TODO_AFTER_REVIEW. Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 30. `intermediate-variables` — flag du code peu imbriqué

**Source :** `mod.rs:1480`
**Observation :**
```
src/rules/no_skipped_test_without_link/rust.rs:19:9:
warning [intermediate-variables] Expression is deeply nested
```
Code flaggé :
```rust
impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "attribute_item" {
                return;
            }
        // …
```
Pas vraiment "deeply nested".

**Décision :** _à compléter_

---

## 31. `justify-inaction` — TODO_AFTER_REVIEW

**Source :** `mod.rs:1491`
**Observation :** marquée TODO_AFTER_REVIEW. Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 32. `no-hidden-control-flow` — flag 2 conditions avec `&&`

**Source :** `mod.rs:1493`
**Observation :** flagged alors qu'il n'y a que 2 conditions :
```rust
if !output.status.success() && output.status.code() != Some(1) {
```
Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 33. `consistent-assert` — flag dans le code de test

**Source :** `mod.rs:1563`
**Observation :**
```
src/rules/strings_comparison/typescript.rs:69:9:
warning [consistent-assert] Use `assert_eq!(a, b)` instead of `assert!(a == b)`
```
« ça doit pas flagged dans les tests si ? » Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 34. `catch-error-name` — TODO_AFTER_REVIEW

**Source :** `mod.rs:1560`
**Observation :** marquée TODO_AFTER_REVIEW. Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 35. `no-zero-fractions` — flag `1.0` dans un calcul `f64::EPSILON`

**Source :** `mod.rs:1625`
**Observation :**
```
src/rules/no_magic_array_flat_depth/typescript.rs:40:23:
warning [no-zero-fractions] Don't use a zero fraction in the number
```
Lignes flaggées :
```rust
if (val - 1.0).abs() < f64::EPSILON {
// et
format!("{:>7.1}ms", d.as_secs_f64() * 1000.0)
```
Le `1.0` est requis pour le typage explicite f64.

**Décision :** _à compléter_

---

## 36. `prefer-simple-condition-first` — flag des comparaisons légitimes

**Source :** `mod.rs:1676`
**Observation :** flagged sur :
```rust
if ch as u32 > 0xFFFF || ch == ZWJ {
// et
if b == b'"' || b == b'\'' || b == b'`' {
```
Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 37. `comment-prose-quality` — flag de la rustdoc valide

**Source :** `mod.rs:1887`
**Observation :**
```
src/main.rs:3:1: warning [comment-prose-quality] Lexical illusion: `!` repeated across lines
//! comply — your code will comply.
//!
//! Enforces coding-standards rules via syntactic analysis. Dispatches to oxlint
//! for TS/JS linting, applies custom tree-sitter rules in-process, and unifies
```
C'est de la rustdoc valide (`//!` = doc-comment de module). Règle commentée
pour l'instant.

**Décision :** _à compléter_
