# Performance: AST walk multiplexing

## Le problème

comply lint son propre code source (3 125 fichiers `.rs`) en **~10s** côté engine.
92% du temps total est dans `engine (rs)`.

```
comply --timings src/
  engine (rs)    9789.4ms   ← 92% du total
  clippy          726.0ms   (parallèle, masqué)
  cargo-shear     641.5ms   (parallèle, masqué)
  cargo-modules   635.2ms   (parallèle, masqué)
  clones          390.4ms
  post-filter     232.3ms
```

### Pourquoi c'est lent

Chaque règle AstCheck fait **son propre `walk_tree`** — un parcours cursor complet de l'AST.
La plupart des règles ne s'intéressent qu'à 1-2 `node.kind()` et font un `return` immédiat
sur tous les autres nœuds. Mais le walk visite quand même chaque nœud.

```
169 règles Rust AstCheck × 3 125 fichiers = 528 000 walks complets de l'AST
```

(170 fichiers `rust.rs` au total, dont 169 AstCheck et 1 TextCheck — `filename_naming_convention`)

Chaque walk est `O(N)` où N = nombre de nœuds dans l'AST du fichier.
Pour un fichier Rust de 200 lignes, N ≈ 2 000-5 000 nœuds.
Total : **~1-2 milliards de visites de nœuds** dont >99% sont des early returns.

Le même problème existe côté TS (1 057 fichiers `typescript.rs` + 120 `react.rs`),
mais il est moins visible car les projets TS ont souvent moins de fichiers
que les 3 125 .rs de comply lui-même.

### Commandes pour reproduire les chiffres

```bash
# Nombre de fichiers .rs
find src -type f -name '*.rs' | wc -l                                        # → 3 125

# Nombre de rust.rs backends total
find src/rules -name 'rust.rs' | wc -l                                       # → 170

# Nombre de rust.rs AstCheck (excl TextCheck)
grep -rl 'ast_check!\|impl AstCheck' src/rules/*/rust.rs | wc -l            # → 169

# Distribution des guards simples node.kind() != "..." (Rust)
# Note: ne couvre pas match node.kind(), collect_nodes_of_kinds, helpers, constantes.
grep -rh 'node.kind() !=' src/rules/*/rust.rs \
  | sed 's/.*!= "//' | sed 's/".*//' | sort | uniq -c | sort -rn

# Breakdown par type de walk (Rust)
# NB: les catégories ne sont pas strictement disjointes — certains fichiers
# utilisent ast_check! ET walk_tree, ou walk_tree ET collect_nodes_of_kinds.
grep -rl 'ast_check!' src/rules/*/rust.rs | wc -l                           # → 88 (macro)
grep -rl 'walk_tree' src/rules/*/rust.rs | wc -l                            # → 43 (manual)
grep -rl 'collect_nodes_of_kinds' src/rules/*/rust.rs | wc -l               # → 34

# Même chose pour TS (typescript.rs + react.rs)
grep -rl 'ast_check!' src/rules/*/typescript.rs src/rules/*/react.rs | wc -l  # → 1 047
grep -rl 'walk_tree' src/rules/*/typescript.rs src/rules/*/react.rs | wc -l   # → 67
grep -rl 'collect_nodes_of_kinds' src/rules/*/typescript.rs src/rules/*/react.rs | wc -l  # → 39
```

### Distribution des guards simples `node.kind() !=` (Rust rules)

Ne couvre que le pattern `if node.kind() != "..." { return; }` dans `ast_check!`.
Les règles avec `match node.kind()`, `collect_nodes_of_kinds`, ou helpers ne sont pas comptées.

```
14 règles cherchent  call_expression
11 règles cherchent  macro_invocation
11 règles cherchent  function_item
 7 règles cherchent  if_expression
 6 règles cherchent  binary_expression
 4 règles cherchent  source_file
 4 règles cherchent  let_declaration
 4 règles cherchent  generic_type
...
```

14 règles qui cherchent `call_expression` font chacune un walk complet de l'arbre
pour visiter les mêmes nœuds. Elles devraient partager un seul walk.

### Stubs no-op

2 règles Rust AstCheck retournent `Vec::new()` sans toucher l'arbre :
- `for_loop_increment_sign/rust.rs` — Rust n'a pas de boucle C-style
- `no_useless_increment/rust.rs` — Rust n'a pas de `++`/`--`

Elles sont quand même enregistrées comme `Backend::TreeSitter`. Le parse Rust
est de toute façon invoqué si d'autres règles TreeSitter Rust sont actives,
donc le gain est uniquement la suppression d'appels `check()` inutiles (mineur).
Supprimer ces backends du registre Rust.

---

## La solution : dispatch par node kind

### Principe

Un seul `walk_tree` par fichier. Pour chaque nœud visité, on dispatch
vers les règles intéressées par ce `node.kind()`.

```
Avant : 169 walks × N nœuds = 169N visites (dont >99% early returns)
Après :   1 walk  × N nœuds = N lookups HashMap + Σ visit_node sur nœuds matchés
```

Le coût post-migration n'est pas zéro par nœud : chaque nœud fait un HashMap lookup,
et les nœuds matchés déclenchent toutes les rules intéressées (ex: 14 rules sur
`call_expression` s'exécutent toutes sur chaque `call_expression`). Le gain vient de
l'élimination des ~99% de nœuds non-matchés, pas du dispatch sur les nœuds matchés.

### Changements requis

#### 1. Nouveau trait `AstCheck` (backward compatible)

```rust
// src/rules/backend.rs

pub trait AstCheck: Send + Sync {
    /// Node kinds this rule cares about. `None` = visit every node (legacy).
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        None
    }

    /// Per-file mutable state. Rules that need to accumulate data across nodes
    /// (e.g. collect all function names, then emit) return a Box<dyn Any> here.
    /// The engine creates one per rule per file, passes it to every visit_node
    /// and finish call.
    ///
    /// CONSTRAINT: Any requires 'static — state must contain owned data only
    /// (String, byte offsets, ranges, Vec<(usize,usize)>), never tree_sitter::Node<'tree>
    /// or &str references into ctx.source. Rules that currently use
    /// collect_nodes_of_kinds must store positions/ranges instead of Node refs.
    ///
    /// The downcast in visit_node/finish is checked at runtime, not compile-time.
    /// Every migrated rule MUST have tests that exercise the stateful path to
    /// catch type mismatches early. A wrong downcast panics in tests, not silently
    /// produces empty diagnostics.
    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        None
    }

    /// Called once per matching node during the multiplexed walk.
    /// `state` is the per-file state created by create_state() (None if the
    /// rule returned None).
    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let _ = (node, ctx, state, diagnostics);
    }

    /// Called once after the walk completes. Rules that need two-phase logic
    /// (collect then emit) produce their diagnostics here using accumulated state.
    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let _ = (ctx, state, diagnostics);
    }

    /// Entry point used by test helpers and legacy engine path.
    ///
    /// Default implementation: if interested_kinds() is Some, simulates
    /// the multiplexed dispatch (walk + visit_node + finish) so that
    /// test helpers like run_ts/run_rust work without modification.
    /// Legacy rules override this with their own walk.
    ///
    /// NOTE: this default uses linear kinds.contains() per node — fine for
    /// tests. The engine MUST use the HashMap dispatch table, never this
    /// default, for production performance.
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        // Multiplexed rules: simulate the engine dispatch for tests
        if let Some(kinds) = self.interested_kinds() {
            let mut state = self.create_state();
            let mut diagnostics = Vec::new();
            crate::rules::walker::walk_tree(tree, |node| {
                if kinds.contains(&node.kind()) {
                    self.visit_node(node, ctx, state.as_deref_mut(), &mut diagnostics);
                }
            });
            self.finish(ctx, state, &mut diagnostics);
            return diagnostics;
        }
        // Legacy: no interested_kinds, rule must override this method
        Vec::new()
    }
}
```

Trois catégories de règles :

| Catégorie | Méthodes implémentées | Exemple |
|-----------|----------------------|---------|
| Stateless (80%) | `interested_kinds` + `visit_node` | `rust_no_unwrap`: cherche `call_expression`, émet immédiatement |
| Stateful two-phase | `interested_kinds` + `create_state` + `visit_node` + `finish` | `symmetric_pairs`: phase 1 collecte les `function_item`, finish émet |
| Legacy (shrinking) | `check` seul | Rules trop complexes, migrées plus tard |

#### Règles two-phase : pattern concret

`symmetric_pairs` (collecte toutes les pub fn, puis vérifie les paires manquantes) :

```rust
struct FnInfo { name: String, line: usize, column: usize, span: Option<(usize, usize)> }
struct State { pub_fns: Vec<FnInfo> }

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["function_item"])
    }

    fn create_state(&self) -> Option<Box<dyn Any>> {
        Some(Box::new(State { pub_fns: Vec::new() }))
    }

    fn visit_node(&self, node: Node, ctx: &CheckCtx,
                   state: Option<&mut dyn Any>, _diags: &mut Vec<Diagnostic>) {
        let st = state.unwrap().downcast_mut::<State>().unwrap();
        // extract name + position from node (owned data), push into st.pub_fns
        // Node<'tree> does NOT go into state — only owned String/usize/ranges
    }

    fn finish(&self, ctx: &CheckCtx,
              state: Option<Box<dyn Any>>, diagnostics: &mut Vec<Diagnostic>) {
        let st = state
            .expect("finish called without state")
            .downcast::<State>()
            .ok().expect("State type mismatch — wrong downcast in finish()");
        // emit diagnostics using st.pub_fns (has name, line, column, span)
    }
}
```

`consistent_destructuring` (collecte les destructurations, puis cherche les member access) :

```rust
// interested_kinds = ["variable_declarator", "member_expression"]
// State: Vec<DestructuredDecl { object: String, props: Vec<String>, end_byte: usize }>
//
// visit_node:
//   si variable_declarator → parse le pattern, push dans state.destructured
//   si member_expression  → check contre state.destructured, mais SEULEMENT si
//     node.start_byte() > decl.end_byte. L'impl actuelle (two-pass) fait cette
//     comparaison pour éviter de flagger des accès dans l'initializer ou avant
//     la déclaration. En one-pass, la comparaison reste nécessaire : le walk
//     top-down visite variable_declarator avant member_expression dans le même
//     scope, mais pas si le member_expression est dans l'initializer du
//     destructuring lui-même ou dans un scope précédent.
//
// Pas besoin de finish — les diagnostics sont émis dans visit_node.
```

#### 2. Nouvelle macro `ast_check!`

```rust
// Forme multiplexée — déclare les kinds + visitor, pas de walk interne
ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let callee = node.child_by_field_name("function");
    // ...
}

// L'ancienne forme continue de compiler (interested_kinds = None, check = walk complet)
ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    // ...
}
```

#### 3. Engine dispatch multiplexé

```rust
// src/engine.rs — dans dispatch_backends()

// Structure complète de dispatch_backends après modification.
// Remplace la boucle unique `for (meta, backend) in &active` existante
// (src/engine.rs:178-203).
//
// Conséquence acceptée : l'ordre d'émission change — tous les non-TreeSitter
// diagnostics sont émis avant les TreeSitter. Ça affecte aussi la sortie
// non-JSON (pretty print). Le tri JSON normalise pour la comparaison.

// ── Phase 1: backends non-TreeSitter (inchangé) ──
for (meta, backend) in &active {
    let mut produced = match backend {
        Backend::Text(check) => check.check(&ctx),
        Backend::Oxlint { .. }
        | Backend::Clippy { .. }
        | Backend::Tsc { .. }
        | Backend::Tsgolint { .. } => Vec::new(), // contribuent au config externe
        Backend::TreeSitter(_) => continue,       // traité en phase 2
    };
    if let Some(sev) = config.severity_for(meta.id) {
        for d in &mut produced { d.severity = sev; }
    }
    diagnostics.extend(produced);
}

// ── Phase 2: backends TreeSitter — multiplexé + legacy ──
let ts_checks: Vec<(&RuleMeta, &dyn AstCheck)> = active
    .iter()
    .filter_map(|(meta, backend)| match backend {
        Backend::TreeSitter(check) => Some((*meta, check.as_ref())),
        _ => None,
    })
    .collect();

let (multiplexed, legacy): (Vec<_>, Vec<_>) = ts_checks
    .into_iter()
    .partition(|(_, check)| check.interested_kinds().is_some());

// Build dispatch table: node_kind → Vec<index into multiplexed>
let mut dispatch: HashMap<&str, Vec<usize>> = HashMap::new();
for (i, (_, check)) in multiplexed.iter().enumerate() {
    for kind in check.interested_kinds().unwrap() {
        dispatch.entry(kind).or_default().push(i);
    }
}

// Per-rule state + per-rule diagnostic buckets (for severity overrides)
let mut states: Vec<Option<Box<dyn Any>>> = multiplexed
    .iter()
    .map(|(_, check)| check.create_state())
    .collect();
let mut per_rule_diags: Vec<Vec<Diagnostic>> = vec![Vec::new(); multiplexed.len()];

// UN SEUL walk pour toutes les rules multiplexées — skippé si aucune rule migrée
if let Some(ref t) = tree {
    if !multiplexed.is_empty() {
        walk_tree(t, |node| {
            if let Some(indices) = dispatch.get(node.kind()) {
                for &i in indices {
                    let (_, check) = &multiplexed[i];
                    check.visit_node(node, &ctx, states[i].as_deref_mut(), &mut per_rule_diags[i]);
                }
            }
        });

        // finish() pour les rules stateful + severity override par règle.
        // Per-rule buckets garantissent que chaque règle émet ses diagnostics
        // dans le même ordre qu'avant (visit_node voit les nœuds top-down,
        // finish émet à la fin). L'ordre inter-règle peut changer, mais
        // l'ordre intra-règle est préservé.
        for (i, (meta, check)) in multiplexed.iter().enumerate() {
            check.finish(&ctx, states[i].take(), &mut per_rule_diags[i]);
            if let Some(sev) = config.severity_for(meta.id) {
                for d in &mut per_rule_diags[i] {
                    d.severity = sev;
                }
            }
            diagnostics.extend(per_rule_diags[i].drain(..));
        }
    }

    // Legacy rules — chacune fait son propre walk
    for (meta, check) in &legacy {
        let mut produced = check.check(&ctx, t);
        if let Some(sev) = config.severity_for(meta.id) {
            for d in &mut produced {
                d.severity = sev;
            }
        }
        diagnostics.extend(produced);
    }
}
```

### 4. Migration des règles

**Batch 1** — Règles `ast_check!` avec un seul `node.kind() !=` guard

Rust : 88 / 169 AstCheck = 52%. TS+React : 1 047 / 1 156 AstCheck = 91%.
Toutes langues confondues : 1 135 / 1 325 = **~86%**.

```bash
# Vérifier ces chiffres
grep -rl 'ast_check!' src/rules/*/rust.rs | wc -l               # → 88
grep -rl 'ast_check!\|impl AstCheck' src/rules/*/rust.rs | wc -l  # → 169
grep -rl 'ast_check!' src/rules/*/typescript.rs src/rules/*/react.rs | wc -l  # → 1 047
grep -rl 'ast_check!\|impl AstCheck' src/rules/*/typescript.rs src/rules/*/react.rs | wc -l  # → 1 156
```

Migration mécanique : retirer le guard, ajouter le kind dans la macro.

```rust
// Avant
ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let callee = node.child_by_field_name("function");
    // ...
}

// Après
ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let callee = node.child_by_field_name("function");
    // ...
}
```

**Batch 2** — Règles avec `collect_nodes_of_kinds` (34 Rust, 39 TS)

Ces règles collectent des nœuds puis itèrent. Deux sous-cas :
- Si l'itération est simple (emit par nœud) → convertir en `visit_node` direct
- Si l'itération a du cross-referencing → utiliser `create_state` + `finish`

**Batch 3** — Règles two-phase avec walk manual (43 Rust, 67 TS+React)

Exemples types :
- `symmetric_pairs` : collecte + émission → `create_state` + `visit_node` + `finish`
- `consistent_destructuring` : multi-kind → `interested_kinds = ["variable_declarator", "member_expression"]`
  + state pour accumuler les destructurations

**Stubs no-op** — 2 règles (`for_loop_increment_sign`, `no_useless_increment`).
Supprimer le backend Rust du registre (`mod.rs`), garder le fichier `rust.rs` si les tests
documentent le "pourquoi pas de backend Rust".

---

## Impact estimé

Le gain dépend du taux de migration :

| Taux de migration | Walks par fichier | Gain estimé engine (rs) |
|-------------------|-------------------|------------------------|
| 0% (baseline) | 169 | — |
| Batch 1 seul (Rust: 52%, all: 86%) | 1 multiplexé + ~81 legacy (Rust) | **~2x** (Rust seul) |
| Batch 1+2 | 1 multiplexé + ~47 legacy (Rust) | **~3-4x** (Rust seul) |
| 100% | 1 multiplexé | **~15-20x** (objectif) |

Estimation proportionnelle au nombre de walks éliminés. Hypothèse : chaque walk
coûte le même temps. En pratique, les rules legacy (batch 3) peuvent être plus
chères que les rules `ast_check!` simples — à valider par `--timings` après chaque batch.

L'objectif 15-20x est atteignable après migration quasi complète.
Le batch 1 donne un gain plus modeste côté Rust (52% des rules migrées)
que côté TS (89%). Les gains réels seront mesurés par `--timings` après chaque batch.

### Gains secondaires (hors scope de ce chantier)

| Optimisation | Impact estimé |
|-------------|---------------|
| `clones` (390ms) | À profiler séparément |
| `post-filter` (232ms) | `is_self_reference` fait 2 allocations/diagnostic — peut être `O(1)` |
| `all_rule_defs()` → `OnceLock` | ~1ms, négligeable |
| Mutex caches → `DashMap` | Meilleur parallélisme, impact inconnu sans contention mesurée |

---

## Critères d'acceptation

### 1. Identité des diagnostics

Après chaque batch de migration, comparer la sortie `--json` entre la branche
legacy (toutes les rules en `check()`) et la branche multiplexée :

```bash
# Deux worktrees pour comparer sans stash ni risque de perte
git worktree add /tmp/comply-baseline main
cd /tmp/comply-baseline && cargo build --release
./target/release/comply --json src/ \
  | jq '[.[] | {path,line,column,ruleId,message,severity}] | sort_by(.path,.line,.column,.ruleId,.severity,.message)' \
  > /tmp/baseline.json

# Branche perf (worktree principal)
cd /Users/rbaumier/www/comply && cargo build --release
./target/release/comply --json src/ \
  | jq '[.[] | {path,line,column,ruleId,message,severity}] | sort_by(.path,.line,.column,.ruleId,.severity,.message)' \
  > /tmp/multiplexed.json

# Normaliser les chemins absolus si comply les émet (worktrees ont des racines différentes)
sed -i '' "s|/tmp/comply-baseline/||g" /tmp/baseline.json
sed -i '' "s|$(pwd)/||g" /tmp/multiplexed.json
diff /tmp/baseline.json /tmp/multiplexed.json
git worktree remove /tmp/comply-baseline
```

Critère : **diff vide** (mêmes diagnostics, mêmes severities, mêmes positions).
Le tri est total (path + line + column + ruleId + severity + message) et couvre tous les champs
exposés par `format_json` (`--json`). L'ordre d'émission peut changer (rules multiplexées
vs séquentielles), d'où la normalisation.

Note : `--json` n'expose pas les byte spans (`Diagnostic::span`). Si la migration
change la façon dont les positions sont calculées (ex: `at_node` vs offsets manuels),
le JSON ne le détectera que si line/column changent. Pour les spans, ajouter un test
Rust interne `assert_multiplexed_matches_legacy` dans `src/rules/test_helpers.rs`
qui compare les `Vec<Diagnostic>` complets (incluant `span`) avant formatage —
**obligatoire pour batch 2-3** où les rules changent de source de position
(collect_nodes_of_kinds → owned offsets, walks manuels → visit_node).
Le surlignage peut changer sans affecter line/column.

### 2. Benchmark par phase de migration

Mesurer `--timings` après chaque batch, sur le corpus comply (`src/`):

| Étape | engine (rs) attendu | Vérifié |
|-------|--------------------|---------| 
| Baseline | ~9 800ms | |
| Infra seule (trait + engine, 0 rules migrées) | ~9 800ms (pas de régression) | |
| Batch 1 (Rust 52%, TS 89%) | ~4 000-5 000ms | |
| Batch 1+2 | ~2 000-3 000ms | |
| Batch 3 (100%) | ~500-800ms | |

Si un batch n'atteint pas la cible, profiler avant de continuer.

---

## Plan d'exécution

1. **Modifier le trait `AstCheck`** — ajouter `interested_kinds` + `create_state` + `visit_node` + `finish` avec defaults. Le default de `check()` simule le dispatch multiplexé quand `interested_kinds` est `Some` — les test helpers (`run_ts`, `run_rust`, etc. dans `src/rules/test_helpers.rs`) appellent `check.check(&ctx, &tree)` et continuent de fonctionner sans modification.
2. **Modifier `ast_check!`** — nouvelle forme `on [kinds]` qui génère `interested_kinds` + `visit_node`
3. **Modifier `dispatch_backends`** — dispatch table, un seul walk, per-rule buckets pour severity overrides, legacy fallback. Backends non-TreeSitter (Text, Oxlint, Clippy, Tsc, Tsgolint) inchangés.
4. **Vérifier** — `--json` diff vide, `--timings` pas de régression, `cargo nextest run` vert (prérequis : `cargo install cargo-nextest`)
5. **Migrer batch 1** — règles `ast_check!` avec guard simple (~80%, mécanique)
6. **Mesurer** — `--timings` + `--json` diff
7. **Migrer batch 2** — `collect_nodes_of_kinds` → visitor (state = owned positions, pas de `Node<'tree>`). Test Rust interne obligatoire pour vérifier les spans.
8. **Mesurer**
9. **Migrer batch 3** — walks manuels + two-phase → `create_state` + `finish`. Test Rust interne obligatoire pour les spans.
10. **Mesurer**
11. **Cleanup** — supprimer stubs no-op du registre, retirer le legacy path si 100% migré
