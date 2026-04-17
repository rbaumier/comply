
```rust
if let Some(os) = object_storage {
    let key = format!(
        "{}/event/{}/{event_id}",
        application_id,
        received_at.naive_utc().date(),
    );
    match timeout(
        S3_TIMEOUT,
        os.client
            .get_object()
            .bucket(&os.bucket)
            .key(&key)
            .send(),
    )
    .await
    {
        Ok(Ok(obj)) => match timeout(S3_TIMEOUT, obj.body.collect()).await {
            Ok(Ok(ab)) => return Some(ab.to_vec()),
            Ok(Err(e)) => {
                log_object_storage_error_with_context!(
                    "S3 GET OBJECT body collect failed",
                    error_chain = format!("{e}"),
                    object_key = &key,
                );
            }
            Err(_) => {
                log_object_storage_error_with_context!(
                    "S3 GET OBJECT body collect timed out",
                    error_chain = "timeout".to_string(),
                    object_key = &key,
                );
            }
        },
        Ok(Err(e)) => {
            log_object_storage_error_with_context!(
                "S3 GET OBJECT failed",
                error_chain = DisplayErrorContext(&e).to_string(),
                object_key = &key,
            );
        }
        Err(_) => {
            log_object_storage_error_with_context!(
                "S3 GET OBJECT timed out",
                error_chain = "timeout".to_string(),
                object_key = &key,
            );
        }
    }
}
```
-----------
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AttemptTrigger {
    #[default]
    Dispatch,
    AutoRetry,
    ManualRetry,
}

impl std::fmt::Display for AttemptTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dispatch => write!(f, "dispatch"),
            Self::AutoRetry => write!(f, "auto_retry"),
            Self::ManualRetry => write!(f, "manual_retry"),
        }
    }
}

impl std::str::FromStr for AttemptTrigger {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dispatch" => Ok(Self::Dispatch),
            "auto_retry" => Ok(Self::AutoRetry),
            "manual_retry" => Ok(Self::ManualRetry),
            _ => Err(()),
        }
    }
}
```
=> il faut utiliser strum et ne pas faire les impl manuellement

--------

```rust
  match replayed {
          Some(event) => {                                                    // level 1
              if let Some(pulsar) = &state.pulsar {                           // level 2
                  let payload = match event.payload {                         // level 3
                      Some(payload) => Some(payload),
                      None => match &state.object_storage {                   // level 4
                          Some(object_storage) => crate::event_payload::fetch_s3_event_payload(
                              object_storage, body.application_id, event_id, event.received_at,
                          ).await,
                          None => None,
                      },
                  };

                  if let Some(p) = payload {                                  // level 4
                      send_request_attempts_to_pulsar(
                          &mut tx,
                          pulsar,
                          body.application_id,
                          event_id,
                          event.received_at,
                          &event.event_type,
                          &p,
                          &event.payload_content_type,
                      )
                      .await?;

                      tx.commit().await?;
                      report_replayed_events(1);
                      Ok(NoContent)
                  } else {                                                    // level 4
                      tx.rollback().await?;
                      Err(Hook0Problem::InternalServerError)
                  }
              } else {                                                        // level 2
                  tx.commit().await?;
                  report_replayed_events(1);
                  Ok(NoContent)
              }
          }
          None => Err(Hook0Problem::NotFound),                                // level 1
      }
```

doit être refacto en :

```rust
 let event = replayed.ok_or(Hook0Problem::NotFound)?;

      let Some(pulsar) = &state.pulsar else {
          tx.commit().await?;
          report_replayed_events(1);
          return Ok(NoContent);
      };

      // Use the inline DB payload, or fall back to S3 if it was offloaded.
      let payload = match (event.payload, &state.object_storage) {
          (Some(payload), _) => payload,
          (None, Some(storage)) => crate::event_payload::fetch_s3_event_payload(
              storage, body.application_id, event_id, event.received_at,
          )
          .await
          .ok_or(Hook0Problem::InternalServerError)?,
          (None, None) => return Err(Hook0Problem::InternalServerError),
      };
```

---

2 fois presque exactement la meme fonction dans 2 fichiers différente ne doit pas passer

---

bug:
src/main.rs:50:1: error [cognitive-complexity] Cognitive complexity is 6 (threshold 5). Simplify this function.
fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::from(1),  // violations found
        Ok(false) => ExitCode::from(0), // clean
        Err(e) => {
            eprintln!(
                "comply: crashed unexpectedly: {e:#}\n\
                 Re-run with RUST_BACKTRACE=1 and report at https://github.com/rbaumier/comply/issues"
            );
            ExitCode::from(2)
        }
    }
}
->
La complexité cognitive de ce code est de 1.
Le détail :
- match : +1 point.
- Les branches du match (Ok(true), Ok(false), Err(e)) n'ajoutent aucun point supplémentaire selon la métrique standard (SonarQube).
C'est un code parfaitement simple et lisible, il n'y a pas besoin de le refactoriser.

+ les tests de la règle sont pourries, il doit y avoir des tests qui s'assure que la complexité calculée est la bonne. Et les tests doivent être partagés entre le TS et le rust

Ton code trouve un score de 5 à cause de ta constante `FLOW_KINDS`. Elle contient des nœuds de l'AST (Abstract Syntax Tree) qui ne devraient pas être comptabilisés. 

Dans la spécification standard de la **Cognitive Complexity** (créée par SonarSource) :
1. L'expression `match` (l'équivalent du `switch`) ajoute **+1 point**.
2. Les branches individuelles (`=>`) n'ajoutent **aucun point**. 
3. Les appels de fonctions (`ExitCode::from`) et de macros (`eprintln!`) n'ajoutent **aucun point**.

### Le problème dans ton implémentation
Si ton algorithme renvoie 5, c'est que `FLOW_KINDS` intercepte probablement les nœuds `match_arm` (les branches de ton match), ou possiblement les macros et retours. 

Avec ton code, si `match_arm` est compté, l'imbrication (`nesting`) s'applique :
* `match_expression` : +1 (avec `nesting` 0)
* Chaque branche `match_arm` est lue au niveau de `nesting` 1. Si tu les comptes, ça ajoute 1 (base) + 1 (nesting) = 2 points par branche.

### La solution pratique
Pour que ton code soit conforme et te renvoie bien **1** sur l'exemple précédent, tu dois nettoyer ta constante `FLOW_KINDS`.

Elle ne doit contenir **que** les nœuds de structure de contrôle réels :

```rust
const FLOW_KINDS: &[&str] = &[
    "if_expression",
    "else_clause",
    "match_expression",
    "for_expression",
    "while_expression",
    "loop_expression",
    // Dans la spécification complète, break/continue comptent 
    // UNIQUEMENT s'ils sautent vers un label (ex: break 'outer).
];
```

**À retirer absolument de `FLOW_KINDS` si tu les as :**
- `match_arm`
- `macro_invocation`
- `call_expression`
- `return_expression`

Le reste de ta logique (la gestion de `nest_increase` et du `else if` direct) est correcte et bien pensée pour Tree-sitter.

---

bug:
src/main.rs:101 :5: warning [no-small-switch] `match` has only 2 arm(s) — use `if/else` instead.

match action {
    ConfigAction::Init { force } => {
        let cwd = std::env::current_dir()?;
        let target = cwd.join(config::CONFIG_FILE_NAME);
        if target.exists() && !force {
            eprintln!(
                "comply: {} already exists — pass --force to overwrite",
                target.display()
            );
            return Ok(());
        }
        std::fs::write(&target, Config::print_default_toml())?;
        println!("comply: wrote {}", target.display());
    }
    ConfigAction::Print => {
        print!("{}", Config::print_default_toml());
    }

-> c'est un match pas un switch 

---

bug:
src/main.rs:189:1: warning [regex-no-duplicate-chars] Duplicate character in regex character class — remove the redundant character.
-> exemple de lignes qui flag :
- discovered: &[SourceFile],
- /// per-glob `disable = [...]` overrides — they run their full lint set
- fn lint_rust(rs_files: &[&SourceFile], config: &Config) -> Result<Vec<Diagnostic>> {
- #[derive(Debug)]

---

bug:
src/main.rs:57:1: warning [regex-sort-flags] Regex flags are not sorted alphabetically — reorder them (e.g. `dgimsvy`).
-> lignes qui flag :
- Re-run with RUST_BACKTRACE=1 and report at https://github.com/rbaumier/comply/issues" <- dans un println!
- https://docs.anthropic.com/en/docs/claude-code" <- dans un format!

---

bug:
src/main.rs:91:25: warning [regex-no-misleading-capturing-group] Capturing group with alternation and quantifier is misleading — the capture may match different things.
-> ligne : .map_err(|e| anyhow::anyhow!("failed to start tokio runtime: {e}"))?;

---

bug:
src/main.rs:57:67: warning [regex-no-non-standard-flag] Non-standard regex flag detected — standard flags are: d, g, i, m, s, u, v, y.
sur Re-run with RUST_BACKTRACE=1 and report at https://github.com/rbaumier/comply/issues" dans un println!
et sur https://docs.anthropic.com/en/docs/claude-code" dans un format!

---

bug:
src/main.rs:57:67: warning [regex-no-useless-flag] Regex flag has no effect on this pattern — remove it.
sur Re-run with RUST_BACKTRACE=1 and report at https://github.com/rbaumier/comply/issues" dans un println!
et sur https://docs.anthropic.com/en/docs/claude-code" dans un format!

---

bug:
src/main.rs:103:44: warning [regex-no-useless-quantifier] Useless quantifier — it can only match once or matches an empty element.
sur let cwd = std::env::current_dir()?;

---
src/main.rs:112:13: warning [no-non-literal-fs-filename] Filesystem operation with non-literal path — validate the path first.
line -> std::fs::write(&target, Config::print_default_toml())?;
je pense pas que ça ait du sens de la garder si ?

---

src/main.rs:110:17: warning [blank-line-between-blocks] Add a blank line before `return`.
eprintln!(
    "comply: {} already exists — pass --force to overwrite",
    target.display()
);
return Ok(());

src/main.rs:105:13: warning [blank-line-between-blocks] Add a blank line between declarations and logic.
let target = cwd.join(config::CONFIG_FILE_NAME);
if target.exists() && !force {

-> je sais pas si ça a du sens de les garder ? ou alors il faut être plus précis ?

---

src/main.rs:343:1: warning [justify-inaction] Early `return;` without an explaining comment — add a comment on the preceding line.
fn report_diagnostics(diagnostics: &[Diagnostic]) {
    if diagnostics.is_empty() {
        println!("comply: all clear");
        return;
    }
    let formatted = output::format_eslint(diagnostics);
    print!("{formatted}");
    eprintln!(
        "\ncomply: {} violation{} found",
        diagnostics.len(),
        if diagnostics.len() == 1 { "" } else { "s" }
    );
}
-> pourquoi on aurait besoin d'un commentaire ?

---

src/main.rs:54:9: warning [catch-error-name] Error binding `e` should be named `error` (or `err`, `_`).
-> c'est idiomatic en rust de faire un Err(e) dans un match result non ?

---

src/main.rs:3:1: warning [comment-prose-quality] Lexical illusion: `!` repeated across lines.
//! comply — your code will comply.
//!
//! Enforces coding-standards rules via syntactic analysis. Dispatches to oxlint
//! for TS/JS linting, applies custom tree-sitter rules in-process, and unifies

-> c'est de la rustdoc valide, ne devrait pas flag

---
src/main.rs:87:1: warning [comment-prose-quality] Weasel word `actually` in comment — be specific.
// Spin up a small tokio runtime for the LSP server.
// Comply itself is sync; we don't pay the runtime cost
// unless the user actually starts the LSP.
-> pourquoi on ne peut pas utiliser actually ?

---
src/main.rs:213:1: warning [comment-prose-quality] Lexical illusion: `/` repeated across lines.
/// We need this post-filter because oxlint/clippy don't know about
-> pourquoi on ne peut pas utiliser "/" ?

---

plein de duplicated blocks alors que c'est pas le cas :
src/rules/sql_no_timestamp_without_tz/mod.rs:14:187: warning [no-clones] Duplicated block (18 lines) — also appears in `src/rules/sql_no_varchar/mod.rs` at line 14. Three similar snippets are a Rule of Three signal: extract a shared helper. Two clones can wait, but if a third appears, refactor.
file 1 :
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-timestamp-without-tz",
    description: "`TIMESTAMP` without timezone — use `TIMESTAMPTZ` to avoid timezone bugs.",
    remediation: "Replace `TIMESTAMP` with `TIMESTAMPTZ` (or `TIMESTAMP WITH TIME ZONE`). Without timezone info, the same instant is interpreted differently depending on the server's `timezone` setting.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],
};
file 2 :
pub const META: RuleMeta = RuleMeta {
    id: "sql-no-varchar",
    description: "`VARCHAR(N)` / `CHAR(N)` — use `TEXT` with a CHECK constraint instead.",
    remediation: "Replace `VARCHAR(N)` with `TEXT` + `CHECK(length(col) <= N)`. VARCHAR's length limit provides no performance benefit in PostgreSQL and silently truncates in some contexts.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],
};

---

serde/zod : voir si il y a des règles serde qu'on peut appliquer à zod et inversement (e.g. rust-serde-deny-unknown-fields)

---

todo-needs-issue-link a l'air pété

---

no-inconsistent-returns a l'air pété

---

https://news.ycombinator.com/item?id=47673171
https://github.com/etechlead/token-map
