# comply — instructions pour Claude

## Workflow

- **Tu peux merger les PR ouvertes par toi-même sans demander de review.** Le user te fait confiance pour l'auto-merge sur ce repo. Préférer `gh pr merge --rebase --delete-branch` pour garder un historique linéaire (le repo n'a pas de merge commits, le user committe directement sur main d'habitude).
- Pour les fixes de FP de règles : un commit par issue, message qui ferme l'issue (`Closes #N`), test de régression qui reproduit l'exemple de l'issue.
- Si une règle s'avère structurellement fausse (mauvaise prémisse), tu peux la supprimer entièrement plutôt que d'ajouter des escape-hatches. Exemples passés : `auth-on-mutation`, `no-raw-db-entity-in-handler`, `drizzle-returning-on-insert-update`, `better-result-prefer-unwrap`.

## Tests

- `cargo test rules::<rule_name>` pour itérer sur une règle isolée.
- `cargo test` pour la full suite avant de push.
- Pas besoin de `cargo build --release` — `cargo run` / `cargo test` suffisent.
- Run timeout des projets dans `test-projects/` : 60s. Au-delà = bug comply, pas un projet trop gros.

## Conventions règles

- **Rust rules : AstCheck uniquement** (tree-sitter), pas de TextCheck — trop de FPs.
- **Defaults thresholds** : `src/config/defaults.toml` est la single source of truth. Les règles `panic!` si la clé manque, pas de fallback côté code.
- **Docblock de règle** : décrit le comportement courant uniquement, jamais l'historique ("previously did X" est interdit, ça vit dans `git log`).
- **Commit message** : la rationale du fix va dans le commit message + le test de régression, pas dans `RULES_TO_FIX.md`.

## Quoi NE PAS faire

- Pas de `pkill` ou `kill` par nom — le user a des sessions actives, n'arrêter QUE les PIDs qu'on a soi-même lancés.
- Pas de désactivation de règle pour faire passer un test — fixer la règle ou changer le code à tester.
- Pas d'agents en parallèle — un à la fois pour ne pas brûler l'usage.
- Pas de comply en parallèle sur plusieurs projets — séquentiel.

## Suppression d'issues sur GitHub

Pour les FP triés et fixés : le commit avec `Closes #N` les ferme automatiquement au merge du PR. Pour les doublons : `gh issue close N --comment "Duplicate of #M — ..."`.
