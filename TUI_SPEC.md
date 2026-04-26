# `comply --tui` — Spec d'implémentation

## Objectif

Mode de navigation interactif ratatui pour explorer des milliers de diagnostics. Le TUI remplace le renderer pretty/eslint quand `--tui` est passé, après que le lint ait terminé.

## Pipeline

```
CLI parse → lint (rayon) → Vec<Diagnostic> → si --tui → charger sources → TUI
                                            → sinon  → pretty/eslint/json
```

1. Le lint tourne normalement et produit un `Vec<Diagnostic>`
2. Si 0 diagnostics : pas de TUI, comportement actuel ("all clear"), exit 0
3. Si stdout n'est pas un TTY (`!std::io::stdout().is_terminal()`) : erreur `"--tui requires an interactive terminal"`, exit 2
4. On déduplique les paths, on `fs::read_to_string` chaque fichier. Si la lecture échoue, on insère `""` (le snippet affichera `<source unavailable>`)
5. On pré-calcule les haystacks de recherche (voir section Recherche)
6. On lance le TUI

Le dispatch se fait dans `lint_project` (`main.rs`), **après** les suppressions et le diff-only filter, **à la place de** la branche `if cli.should_emit_json { … } else { report_diagnostics(…) }` (autour de la ligne 240). Le TUI est un troisième bras du `if`/`else` de rendu.

## CLI

```rust
#[arg(long, conflicts_with_all = ["should_emit_json", "fix"])]
pub tui: bool,
```

Ajouté sur le struct `Cli` à côté de `should_emit_json`. Combinable avec tous les scan modes (`--staged`, `--working-tree`, etc.) et avec `--diff-only`, `--timings`.

Mutuellement exclusif avec `--json` et `--fix` via `conflicts_with_all`.

Pour les subcommands (`explain`, `list`, `catalog`, `config`, `lsp`) : pas de conflit clap — `--tui` est ignoré quand un subcommand est présent. Le check se fait **en haut de `run()`**, avant le `match cli.command` :

```rust
if cli.tui && cli.command.is_some() {
    eprintln!("warning: --tui is ignored when a subcommand is specified");
}
```

`--tui` ne s'applique qu'au mode lint par défaut (branche `cli.command == None`).

Exit code inchangé : 0 (clean), 1 (violations), 2 (crash). En mode `--tui`, le fait que l'utilisateur quitte avec `q` ne change pas l'exit code — si des violations existent, c'est exit 1. `tui::run()` ne modifie pas la valeur de retour de `lint_project` (`Ok(true)` = violations trouvées).

## Architecture du code

```
src/tui/
├── mod.rs          // point d'entrée
├── app.rs          // App state
├── ui.rs           // fn draw(frame, app) — layout ratatui
└── event.rs        // Event loop : crossterm events → App mutations
```

### Signature du point d'entrée

```rust
pub fn run(diagnostics: Vec<Diagnostic>, sources: HashMap<PathBuf, String>) -> anyhow::Result<()>
```

### Modèle de données — index-based

Les diagnostics sont stockés dans un `Vec<Diagnostic>` owned par `App`. Toutes les vues (by-file, by-rule) et le filtre référencent les diagnostics par **index `usize`** dans ce Vec, évitant les problèmes de lifetime et de self-referential structs. Aucun clone de `Diagnostic` n'est nécessaire — tout accès passe par `&self.diagnostics[idx]`.

```rust
struct App {
    diagnostics: Vec<Diagnostic>,
    sources: HashMap<PathBuf, String>,
    haystacks: Vec<String>,          // lowercased search corpus, 1:1 avec diagnostics

    view_mode: ViewMode,             // ByFile | ByRule
    by_file: BTreeMap<PathBuf, Vec<usize>>,   // path → indices
    by_rule: BTreeMap<String, Vec<usize>>,    // rule_id → indices

    // Summaries pré-calculées, recalculées au changement de filtre ou de vue
    group_summaries: HashMap<String, GroupSummary>,

    cursor: usize,                   // position dans la liste visible (rows)
    expanded_groups: HashSet<String>, // clés des groupes unfoldés
    expanded_diags: HashSet<usize>,  // indices des diagnostics explain-expanded

    input_mode: InputMode,           // Normal | Search
    search_query: String,
    filtered_indices: Option<Vec<usize>>, // None = pas de filtre actif

    pending_g: bool,                 // pour le chord `gg`
    status_message: Option<String>,  // messages temporaires, effacés au prochain keystroke
}

struct GroupSummary {
    total: usize,
    errors: usize,
    warnings: usize,
    file_count: usize, // nb fichiers distincts (utile pour vue by-rule)
}
```

### Notes sur le modèle

- **`Diagnostic` n'est pas `Clone`** — c'est intentionnel, tout passe par `&self.diagnostics[idx]`. Ne pas ajouter `Clone` à `Diagnostic`.
- **`GroupSummary`** porte `errors`/`warnings` même en vue by-rule (où un seul sera non-zéro) — symétrie intentionnelle pour éviter un enum de summary.
- **`status_message`** est effacé au prochain keystroke (pas de timeout).
- **Clés de `group_summaries`** : `path.display().to_string()` en vue by-file, `rule_id.clone()` en vue by-rule. Les paths non-UTF8 ne sont pas supportés (acceptable — comply est un linter de code source).
- **Tests existants** : `cli.rs` contient un test `cli_with_defaults()` qui construit `Cli { .. }` littéralement — il faudra ajouter `tui: false` à ce test (et tout autre constructeur littéral de `Cli`).

### Mémoire

Le chargement upfront des sources est borné par le nombre de fichiers ayant des diagnostics. Pour des centaines de fichiers (ordre de grandeur confirmé), c'est quelques dizaines de MB max — acceptable. Un lazy-loading est hors scope mais envisageable en futur si les monorepos posent problème.

### Dépendances

Ajoutées dans `Cargo.toml` (non optionnelles) :
- `ratatui`
- `crossterm`
- `unicode-width` (alignement des carets sur source non-ASCII)

### Threading

Le TUI est entièrement **synchrone** (single-threaded). L'event loop bloque sur `crossterm::event::poll`. Pas de tokio.

## Layout

```
┌──────────────────────────────────────────────────────────┐
│ comply --tui   [◉ By file] [○ By rule]  42/1837 violations│  ← status bar
├──────────────────────────────────────────────────────────┤
│ ▶ src/api/handler.rs           12 (3 err, 9 warn)        │
│ ▼ src/api/router.rs             2 (2 warn)               │
│   ⚠ 14:5  unused import `foo`                            │
│   ✖ 31:12 missing error handling                         │
│     │ let result = db.query(sql);                         │  ← explain expanded
│     │              ^^^^^^^^^^^^^^^                        │
│     help: wrap in Result or use `?` operator              │
│ ▶ src/lib.rs                    1 (1 err)                │
│                                                           │
├──────────────────────────────────────────────────────────┤
│ ↑↓ navigate  →← fold  Enter open  / search  Tab view  q quit│  ← help bar
└──────────────────────────────────────────────────────────┘
```

### Status bar (haut)

Deux onglets visibles (`◉`/`○`), compteur `filtré/total violations`. Les glyphes `◉`/`○` sont safe — comply requiert déjà un terminal unicode (le pretty renderer utilise `GraphicalTheme::unicode()`).

### Help bar (bas)

Toujours visible. Remplacée par l'input `/` en mode recherche.

### Troncature

Lignes tronquées avec `…` si le terminal est trop étroit. Pas de wrapping.

## Vues

Deux modes, toggle via `Tab` (mutually exclusive) :

| Mode | Groupement | Header foldé |
|---|---|---|
| By file (défaut) | `BTreeMap<PathBuf, Vec<usize>>` | `▶ path  N violations (X err, Y warn)` |
| By rule | `BTreeMap<String, Vec<usize>>` | `▶ rule_id  N violations across M files` |

Tous les groupes sont **foldés par défaut**.

### Tri

- **Groupes** : ordre alphabétique (`BTreeMap` sur path ou rule_id)
- **Diagnostics dans un groupe** : triés par `(line, column)` croissant

### Counts dans les headers

Les counts (N violations, X err, Y warn, M files) reflètent le **filtre actif** — cohérent avec le compteur global de la status bar.

Pour la vue "By rule", `M files` = nombre de paths distincts parmi les diagnostics filtrés du groupe. Le breakdown `(X err, Y warn)` n'est pas affiché en vue "By rule" car une règle a une sévérité fixe.

Les `GroupSummary` sont pré-calculées à l'init et recalculées au changement de filtre ou au toggle de vue (pas à chaque render).

### Comportement du toggle `Tab`

Au toggle de vue :
- `cursor` → reset à 0 (premier group header)
- `expanded_groups` → vidé (les clés changent de sémantique entre path et rule_id)
- `expanded_diags` → préservé (les indices diagnostics sont stables entre les vues)
- `filtered_indices` → préservé (le filtre est indépendant de la vue)
- `search_query` → préservé
- `pending_g` → reset à false

## Diagnostic individuel

### Replié (une ligne)

```
  ⚠ 14:5  unused import `foo`
```

`⚠` jaune pour warning, `✖` rouge pour error.

### Expanded (via `→`) — expansion en place

```
  ⚠ 14:5  unused import `foo`
    │ import { foo, bar } from './utils';
    │          ^^^
    help: Remove the unused import
    url: https://...
```

**Snippet source** : uniquement la ligne contenant le diagnostic (0 lignes de contexte). On utilise `diag.line` pour extraire la ligne du fichier source (split par `\n`, index `line - 1`). Le caret (`^`) est positionné à `diag.column` et couvre :
- Si `diag.span` est `Some((offset, len))` : `len` caractères (capé à la fin de la ligne)
- Si `diag.span` est `None` : toute la ligne à partir de `column`

Utilise `unicode-width` pour l'alignement du caret sur source non-ASCII/tabs. Le TUI implémente son propre rendu de snippet — il ne réutilise **pas** `miette::GraphicalReportHandler` de `output/pretty.rs` (miette rend en string, pas en widgets ratatui).

**Champs `help:` et `url:`** : proviennent de `meta_registry::lookup(rule_id: &str) -> Option<RuleMeta>` (signature : `pub fn lookup(rule_id: &str) -> Option<RuleMeta>`, retourne une copie). Le label `help:` affiche `meta.remediation`, le label `url:` affiche `meta.doc_url`.

**Fallbacks** :
- Span multi-ligne → caret sur la première ligne du span uniquement
- Source indisponible (`""` dans le cache) → affiche `<source unavailable>` au lieu du snippet
- `diag.line` hors bornes du fichier source → affiche `<source unavailable>`
- `meta_registry::lookup` retourne `None` (diagnostics délégués oxlint/clippy) → pas de `help:` ni `url:`
- `meta.doc_url` est `None` → pas de ligne `url:` (seul `help:` est affiché si `remediation` est présent)

## Recherche

- **`/`** active le mode recherche : l'input remplace la help bar
- **Filtre temps réel** : à chaque keystroke, filtre par case-insensitive `contains` sur les haystacks pré-calculés
- Les groupes vides après filtrage disparaissent
- Le compteur devient `42/1837 violations`
- **`Esc`** annule la recherche et restaure la vue complète
- **`Enter`** valide le filtre, ferme l'input, retourne en mode liste (le filtre reste actif jusqu'au prochain `/` ou `Esc` en mode liste)

### Haystacks pré-calculés

À l'init, pour chaque diagnostic `i`, on construit :

```rust
fn build_haystack(diag: &Diagnostic, sources: &HashMap<PathBuf, String>) -> String {
    let source_line = sources
        .get(&diag.path)
        .and_then(|s| s.lines().nth(diag.line.saturating_sub(1)))
        .unwrap_or("");
    format!(
        "{} {} {} {}",
        diag.path.display(),
        diag.rule_id,
        diag.message,
        source_line,
    ).to_lowercase()
}
```

Le filtre teste `haystack.contains(&query.to_lowercase())` — O(n) sur le nombre de diagnostics, instantané pour quelques milliers d'entrées en Rust.

## Keybindings

### Mode liste (Normal)

| Touche | Contexte | Action |
|---|---|---|
| `↑` / `k` | Liste | Monter |
| `↓` / `j` | Liste | Descendre |
| `→` / `l` | Groupe foldé | Unfold |
| `→` / `l` | Diagnostic replié | Expand détail (explain) |
| `←` / `h` | Diagnostic expanded | Replier détail |
| `←` / `h` | Diagnostic replié | Fold le groupe parent |
| `←` / `h` | Groupe unfoldé | Fold |
| `←` / `h` | Groupe foldé | Ne fait rien |
| `Enter` | Groupe foldé | Unfold (synonyme de `→`) |
| `Enter` | Groupe unfoldé | Ne fait rien |
| `Enter` | Diagnostic | Ouvrir dans `$EDITOR` |
| `Tab` | Partout | Toggle by file ↔ by rule |
| `/` | Partout | Activer recherche |
| `Esc` | Liste | Ne fait rien |
| `gg` | Liste | Aller au début |
| `G` | Liste | Aller à la fin |
| `q` | Liste | Quitter |

### Mode recherche (Search)

L'input est **append-only** (pas de curseur interne). Touches capturées :

| Touche | Action |
|---|---|
| `Esc` | Annuler + effacer filtre, retour mode liste |
| `Enter` | Valider le filtre, retour mode liste (filtre reste actif) |
| `Backspace` | Supprimer dernier caractère |
| `Ctrl-U` | Effacer tout l'input |
| `/` | Caractère littéral `/` ajouté au query |
| `Tab` | Ignoré (ne s'insère pas, ne toggle pas la vue) |
| Tout autre caractère imprimable | Ajouter au query |

### Chord `gg`

Implémenté via un flag `pending_g: bool` dans `App`. À la réception de `g` :
- Si `pending_g` est true → action "aller au début", reset `pending_g`
- Sinon → set `pending_g = true`

Toute autre touche reset `pending_g`. Pas de timeout — le chord se complète à la prochaine touche quelle qu'elle soit.

### Curseur et sélection

- **Curseur initial** : première ligne visible (premier group header, tout étant foldé)
- **Modèle** : le curseur suit un **index de ligne visible** (row index). Après un fold/unfold, si l'item sous le curseur disparaît, le curseur se repositionne sur le group header parent
- **`→` pour unfold** : le curseur reste sur le group header. `↓` descend dans les enfants
- **`←` pour fold** depuis un enfant : le curseur remonte sur le group header
- **`←` pour collapse** un explain expanded : le curseur reste sur le diagnostic

## Ouverture éditeur

### Résolution de l'éditeur

1. Tenter `$EDITOR`, puis `$VISUAL`
2. Si aucun n'est défini → afficher `"set $EDITOR to open files"` dans la status bar, ne rien faire

### Parsing de la commande

La valeur de `$EDITOR` est **splittée sur les espaces** pour supporter `EDITOR="code --wait"` ou `EDITOR="nvim -u NONE"`. Le premier token est l'exécutable, les suivants sont des args prépendés.

```
$EDITOR = "code --wait"
→ exec: "code", args: ["--wait", "+{line}", "{path}"]
```

### Détection GUI vs terminal

Matching sur le **basename** du premier token (extraction via `Path::file_name`) :
- GUI : `code`, `zed`, `subl`, `sublime_text`, `idea`, `goland`, `webstorm`, `atom`, `cursor`
- Tout le reste → traité comme terminal. La liste est intentionnellement minimale ; les éditeurs inconnus sont traités comme terminal (suspend/restore, qui fonctionne pour tout).

### Comportement

- **GUI** : fork en background (`Command::new(...).spawn()`), le TUI reste au premier plan
- **Terminal** : suspend le TUI (`disable_raw_mode` + `LeaveAlternateScreen`), lance l'éditeur (`Command::new(...).status()`), au retour re-init le TUI (`EnterAlternateScreen` + `enable_raw_mode`)

### Après fermeture de l'éditeur

Pas de re-lint (hors scope). Le snippet affiché peut devenir stale si l'utilisateur a modifié le fichier. Comportement accepté — le TUI reflète l'état au moment du lint.

## Edge cases

| Cas | Comportement |
|---|---|
| 0 diagnostics | Pas de TUI, comportement actuel ("all clear"), exit 0 |
| `--tui` sans TTY (pipe/CI) | Erreur `"--tui requires an interactive terminal"`, exit 2 |
| Recherche sans résultat | Message centré "No matches for «xyz»", compteur `0/N` |
| `$EDITOR` non défini | Message status bar, pas d'action |
| `$EDITOR` avec espaces | Shell-split sur les espaces |
| `--tui --json` | Erreur clap (`conflicts_with_all`) |
| `--tui --fix` | Erreur clap (`conflicts_with_all`) |
| `--tui` + subcommand | Subcommand prend priorité, `--tui` ignoré avec warning stderr |
| Terminal très étroit | Troncature `…` |
| Resize terminal | Géré par ratatui/crossterm, redraw automatique |
| Fichier source illisible | `""` dans le cache, snippet affiche `<source unavailable>` |
| `diag.line` hors bornes | Snippet affiche `<source unavailable>` |
| Span multi-ligne | Caret sur la première ligne du span uniquement |
| `meta_registry::lookup` → None | Pas de `help:` ni `url:` dans l'expand |
| `Enter` sur groupe déjà unfoldé | Ne fait rien |

## Hors scope (futur)

- Mode streaming (diagnostics pendant le lint)
- Actions fix depuis le TUI
- Feature flag / workspace split
- Copier path dans clipboard
- Ouvrir doc_url dans le browser
- Re-lint après édition
- Lazy-loading des sources (si monorepos posent des problèmes mémoire)
- Afficher les `categories` de `RuleMeta` dans la vue par règle
- Curseur interne dans l'input de recherche (line-editing avancé)
