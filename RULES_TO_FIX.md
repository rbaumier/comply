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

## 9. `sql-no-offset-pagination` — flag le mot `offset` dans une liste

**Source :** `mod.rs:996`
**Observation :**
```
src/rules/explicit_units/typescript.rs:21:1:
warning [sql-no-offset-pagination] `OFFSET` pagination is O(N)…
```
Code flaggé :
```rust
const AMBIGUOUS_BASES: &[&str] = &[
    "delay", "timeout", "interval", "duration", "elapsed", "age", "wait",
    "size", "length", "distance", "offset", "width", "height", "limit",
    "rate", "frequency", "threshold",
];
```
Le mot `offset` apparaît comme literal dans une liste de bases ambiguës,
pas dans une requête SQL. Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 10. `sql-no-varchar` — flag dans un test de regex

**Source :** `mod.rs:1004`
**Observation :** flagged sur :
```rust
fn flags_negative_lookahead_same_char() {
    assert_eq!(run(r#"const re = /(?!a)a/;"#).len(), 1);
}
```
Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 11. `db-no-string-concat-sql` — flag des `format!()` qui ne sont pas SQL

**Source :** `mod.rs:1015`
**Observation :**
```
src/oxlint/mod.rs:106:9:
error [db-no-string-concat-sql] String interpolation with SQL keywords
```
Code flaggé :
```rust
format!(
    "failed to parse oxlint JSON output. oxlint stderr: {}",
    String::from_utf8_lossy(stderr)
)
```
Pas du SQL. Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 12. `no-empty-collection-use` — flag `HashMap::new()` ?

**Source :** `mod.rs:1035`
**Observation :** flagged sur :
```rust
let mut rules: HashMap<String, RuleConfig> = HashMap::new();
```
Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 13. `prefer-immediate-return` — flag un parser muté avant return

**Source :** `mod.rs:1051`
**Observation :**
```
src/rules/playwright_no_networkidle/typescript.rs:60:1:
warning [prefer-immediate-return] Variable `parser` is assigned and immediately returned
```
Code flaggé :
```rust
fn run(path: &str, source: &str) -> Vec<Diagnostic> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .unwrap();
    // ...
}
```
Le parser est muté entre l'assignation et le return — pas un cas
"assigned and immediately returned". Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 14. `no-clear-text-protocol` — flag `http://` dans les commentaires

**Source :** `mod.rs:1060`
**Observation :** flag :
```rust
if text.contains("http://") || text.contains("https://")
```
ET aussi les `http://` dans les commentaires.

**Décision :** _à compléter_

---

## 15. `no-duplicate-string` — flag des fragments de schéma JSON commenté

**Source :** `mod.rs:1073`
**Observation :** prend en compte le contenu d'un schéma JSON
qui se trouve à la fois dans des commentaires (rustdoc d'exemples) et
dans une `const UNIFIED_SCHEMA: &str = r#"{ ... }"#;`. Sous-question
laissée : « il faudrait que ce soit déclenché uniquement quand c'est le
seul contexte d'apparition ? » Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 16. ??? — règle non identifiée, question d'exclusion des tests

**Source :** `mod.rs:1106`
**Observation :** `on veut peut-être l'ignorer dans les tests ?` Suit un
extrait de test :
```rust
fn missing_config_falls_back_to_defaults() {
    let tmp = TempDir::new().unwrap();
    let cfg = Config::load_from(tmp.path()).unwrap();
    let _ = cfg.threshold("max-function-lines", "max", 30);
}
```
Le commentaire ne nomme pas la règle déclenchée — **à retrouver**.

**Décision :** _à compléter_

---

## 17. `no-nested-switch` — flag les `match` Rust imbriqués

**Source :** `mod.rs:1119`
**Observation :**
```
src/rules/no_misleading_collection_name/typescript.rs:118:13:
error [no-nested-switch] Nested `match` — extract the inner match
```
Code flaggé :
```rust
fn initializer_shape(value: tree_sitter::Node, source: &[u8]) -> Option<Shape> {
    match value.kind() {
        "array" => Some(Shape::Array),
        "new_expression" => {
            let ctor = value.child_by_field_name("constructor")?;
            let ctor_name = ctor.utf8_text(source).ok()?;
            match ctor_name {
                "Set" => Some(Shape::Set),
                "Map" => Some(Shape::Map),
                "Array" => Some(Shape::Array),
                _ => None,
            }
        }
        _ => None,
    }
}
```
Question : peut-être une règle dédiée `max-match-depth` pour Rust ? Ou
ne rien faire en Rust car la complexité cyclomatic/cognitive couvre déjà ?

**Décision :** _à compléter_

---

## 18. `function-inside-loop` — non pertinent en Rust

**Source :** `mod.rs:1138`
**Observation :** flagged sur :
```rust
for job in &jobs {
    let file_id = job.path.display().to_string();
    let wr = match worker_results.iter().find(|r| r.id == file_id) {
        Some(r) => r,
        None => continue,
    };
}
```
Note que j'ai laissée :
> Ce lint est techniquement invalide en Rust. En Rust, une closure n'est
> pas un objet dynamique. C'est une simple structure anonyme allouée sur
> la pile à coût zéro. Il n'y a aucune allocation sur le tas.

Règle commentée pour l'instant.

**Décision :** _à compléter_

---

## 19. `nested-control-flow` — compte les `impl` comme niveau d'imbrication

**Source :** `mod.rs:1162`
**Observation :** prend aussi les `impl`/etc. — il faudrait que ce soit
uniquement à l'intérieur d'une fonction. Code flaggé :
```rust
impl Language {
    pub fn is_typescript_family(self) -> bool {
        matches!(
            self,
            Language::TypeScript | Language::Tsx | Language::JavaScript
        )
    }
    // …
}
```
Et aussi sur :
```rust
fn classify(path: &Path) -> Option<SourceFile> {
    let ext = path.extension()?.to_str()?;
    let language = if TS_EXTENSIONS.contains(&ext) {
        // …
    } else if /* … */;
}
```

**Décision :** _à compléter_

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
