# Review des règles non auditées

Date: 2026-04-25

Périmètre initial: règles natives présentes sous `src/rules` mais absentes de `audit-rules.md`, principalement les lots ajoutés récemment `css_*`, `dockerfile_*` et `k8s_*`.

Extension après reprise: audit mécanique de tous les dossiers de règles sous `src/rules`, pas seulement CSS/Dockerfile/K8s.

## Méthode

- Comparer dossiers de règles, `pub mod` et appels `register()` dans `src/rules/mod.rs`.
- Vérifier que les backends ciblent le bon langage (`Css`, `Dockerfile`, `Yaml`) et que les fichiers sont détectés par `src/files.rs` / `src/parsing.rs`.
- Lire les tests des règles enregistrées: au minimum cas violation + cas valide; pour les règles multi-ressources, vérifier un vrai scénario cross-file ou multi-manifest.
- Lancer les tests Rust et noter les échecs exacts.
- Classer chaque trouvaille en `BLOCKER`, `ISSUE` ou `MINOR`.

## Progression

- [x] Détection du périmètre non audité.
- [x] Comparaison mécanique dossiers / modules / registry.
- [x] Tests complets.
- [x] Review CSS.
- [x] Review Dockerfile.
- [x] Review Kubernetes.
- [x] Reprise sur toutes les règles du repo.
- [x] Synthèse finale.

## Trouvailles

### BLOCKER: beaucoup de règles ajoutées ne sont pas activées

Comptage effectif au 2026-04-25, hors faux positifs dont le nom contient `css` sans appartenir au lot CSS:

| Lot | Dossiers | `pub mod` | `register()` | Non enregistrées |
| --- | ---: | ---: | ---: | ---: |
| CSS | 30 | 30 | 8 | 22 |
| Dockerfile | 54 | 19 | 19 | 35 |
| Kubernetes | 60 | 35 | 35 | 25 |

Conséquence: une grande partie des règles non auditées ne peut pas fonctionner en production même si leurs tests unitaires existent, car elles ne sont jamais exposées par le registry.

CSS non enregistrées:

- `css_calc_needs_spaces`
- `css_custom_property_needs_var`
- `css_font_family_needs_generic`
- `css_font_family_quotes`
- `css_keyframe_no_duplicate_selectors`
- `css_keyframe_no_important`
- `css_no_deprecated_media_type`
- `css_no_deprecated_property_value`
- `css_no_duplicate_custom_properties`
- `css_no_duplicate_font_family`
- `css_no_duplicate_properties`
- `css_no_empty_block`
- `css_no_empty_comment`
- `css_no_invalid_hex`
- `css_no_invalid_media_query`
- `css_no_nonstandard_gradient_direction`
- `css_no_redundant_longhand`
- `css_no_shorthand_overrides_longhand`
- `css_no_unknown_function`
- `css_no_unknown_media_feature`
- `css_no_unknown_media_value`
- `css_no_unknown_property_value`

Dockerfile non déclarées / non enregistrées:

- `dockerfile_absolute_workdir`
- `dockerfile_add_for_archive_extract`
- `dockerfile_apk_no_cache`
- `dockerfile_apt_clean_lists`
- `dockerfile_apt_get_y_flag`
- `dockerfile_apt_no_recommends`
- `dockerfile_copy_from_known_stage`
- `dockerfile_copy_from_not_self`
- `dockerfile_copy_needs_workdir`
- `dockerfile_copy_trailing_slash`
- `dockerfile_dnf_clean_all`
- `dockerfile_dnf_y_flag`
- `dockerfile_env_no_self_reference`
- `dockerfile_instruction_order`
- `dockerfile_no_add_for_files`
- `dockerfile_no_apt_end_user`
- `dockerfile_no_cd_in_run`
- `dockerfile_no_from_platform`
- `dockerfile_no_maintainer`
- `dockerfile_no_multiple_cmd`
- `dockerfile_no_multiple_entrypoint`
- `dockerfile_no_onbuild_recursion`
- `dockerfile_no_shell_utils_in_run`
- `dockerfile_no_sudo`
- `dockerfile_no_zypper_dist_upgrade`
- `dockerfile_pip_no_cache_dir`
- `dockerfile_pipefail`
- `dockerfile_single_healthcheck`
- `dockerfile_unique_stage_names`
- `dockerfile_valid_port`
- `dockerfile_yarn_cache_clean`
- `dockerfile_yum_clean_all`
- `dockerfile_yum_y_flag`
- `dockerfile_zypper_clean`
- `dockerfile_zypper_non_interactive`

Kubernetes non déclarées / non enregistrées:

- `k8s_deployment_anti_affinity`
- `k8s_hpa_min_three_replicas`
- `k8s_job_ttl_required`
- `k8s_no_allow_privileged_scc`
- `k8s_no_deprecated_extensions_api`
- `k8s_no_deprecated_service_account_field`
- `k8s_no_docker_sock_mount`
- `k8s_no_duplicate_env_vars`
- `k8s_no_exposed_services`
- `k8s_no_host_ipc`
- `k8s_no_host_network`
- `k8s_no_host_pid`
- `k8s_no_privileged_container`
- `k8s_no_privileged_ports`
- `k8s_no_secret_in_env_literal`
- `k8s_no_sensitive_host_mounts`
- `k8s_no_unsafe_proc_mount`
- `k8s_no_unsafe_sysctls`
- `k8s_no_writable_host_mount`
- `k8s_pdb_eviction_policy`
- `k8s_prefer_secret_files_over_env`
- `k8s_rbac_no_cluster_admin_binding`
- `k8s_rbac_no_create_pods`
- `k8s_rbac_no_secret_access`
- `k8s_restart_policy_required`

### MINOR: marqueur temporaire laissé dans `src/rules/mod.rs`

`src/rules/mod.rs` contient `// TEMP-VERIFY: 22 css rules` alors que 30 dossiers CSS existent. Le marqueur est obsolète et signale que l'intégration du lot CSS n'est pas terminée.

Status: corrigé pendant la review. Le marqueur temporaire a été retiré.

### ISSUE: deux tests CSS échouaient au premier passage

Commande: `cargo test`

Résultat initial:

- `rules::css_font_family_quotes::css::tests::flags_unquoted_multi_word`: attendait 1 diagnostic, recevait 0.
- `rules::css_no_deprecated_media_type::css::tests::flags_tv_media_type`: attendait 1 diagnostic, recevait 0.

Cause:

- `css_font_family_quotes` supposait que `Times New Roman` était exposé par tree-sitter comme plusieurs `plain_value`; le parseur peut exposer la valeur autrement.
- `css_no_deprecated_media_type` cherchait des identifiants top-level trop spécifiques dans le `media_statement`.

Status: corrigé pendant la review. `cargo test css_` passe: 101 tests CSS OK.

### FIX: toutes les règles CSS/Dockerfile/K8s sont maintenant raccordées au registry

Après correction:

| Lot | Dossiers | `pub mod` | `register()` | Manquantes |
| --- | ---: | ---: | ---: | ---: |
| CSS | 30 | 30 | 30 | 0 |
| Dockerfile | 54 | 54 | 54 | 0 |
| Kubernetes | 60 | 60 | 60 | 0 |

Note: le comptage brut `rg "css_.*::register"` donne plus que 30 parce qu'il inclut aussi des règles non-CSS dont le nom contient `css`, comme `i18n_prefer_logical_css_properties`.

Validation intermédiaire: `cargo check` passe, avec seulement le warning existant `src/rules/jsdoc_text_helpers.rs:12` (`raw` jamais lu).

### REVIEW: lot Dockerfile activé et suffisamment couvert

Etat final:

- 54 dossiers `dockerfile_*`.
- 54 modules déclarés.
- 54 règles enregistrées.
- Chaque règle Dockerfile vérifiée contient au moins un cas de violation et un cas valide.
- Les règles ciblent bien `Language::Dockerfile` et les tests passent par le parser Dockerfile via `run_dockerfile`.

Minor non bloquant: plusieurs fichiers d'implémentation Dockerfile s'appellent `typescript.rs`. Le contenu cible bien Dockerfile, donc ce n'est pas un bug fonctionnel, mais le nom de fichier rend la maintenance moins lisible.

### BLOCKER: `k8s_no_writable_host_mount` ratait un montage hostPath non monté explicitement

Commande: `cargo test`

Résultat après activation complète des règles:

- `rules::k8s_no_writable_host_mount::text::tests::flags_writable_host_path` échouait.

Cause: la règle considérait un volume `hostPath` comme sûr si aucun `volumeMount` correspondant n'était trouvé. Cela masquait les cas où un volume `hostPath` est déclaré sans montage en lecture seule explicite.

Status: corrigé pendant la review. La règle exige maintenant qu'un mount correspondant existe et que tous les mounts correspondants aient `readOnly: true`. Tests ciblés ajoutés:

- hostPath monté en read-only accepté.
- hostPath monté en écriture signalé.

Validation: `cargo test k8s_no_writable_host_mount` passe: 4 tests OK.

### ISSUE: les règles Kubernetes cross-manifest n'étaient pas assez testées

Plusieurs règles Kubernetes utilisent `K8sIndex` pour résoudre des relations entre manifests, mais leurs tests ne couvraient pas assez les scénarios projet réel avec plusieurs fichiers.

Correction de scope de test:

- `ProjectCtx::for_test_with_files` construit maintenant aussi `K8sIndex`.
- Ajout de helpers de test YAML avec chemin de fichier réel et projet multi-source.
- Ajout de cas positifs et négatifs cross-file pour:
  - `k8s_dangling_hpa`
  - `k8s_dangling_ingress`
  - `k8s_dangling_service`
  - `k8s_non_existent_service_account`
  - `k8s_env_value_from_resolves`
  - `k8s_dangling_network_policy`
  - `k8s_dangling_network_policy_peer`
  - `k8s_dangling_service_monitor`

Validation: `cargo test k8s_` passe: 187 tests OK.

### REVIEW: lot Kubernetes activé et élargi

Etat final:

- 60 dossiers `k8s_*`.
- 60 modules déclarés.
- 60 règles enregistrées.
- Toutes les règles Kubernetes vérifiées ont au moins un test de violation et un test valide.
- Les règles mono-manifest couvrent les cas YAML attendus.
- Les règles multi-ressources ont maintenant de vrais tests projet/cross-file.

Le scope est cohérent pour Kubernetes YAML standard: les règles s'appliquent au niveau des manifests et ne dépendent pas d'un framework applicatif particulier.

### ISSUE: la première passe ne couvrait pas toutes les règles

Après demande de reprise, le périmètre a été élargi à tous les dossiers de règles sous `src/rules`.

Résultat de l'inventaire complet:

- 1419 dossiers sous `src/rules`.
- 1419 dossiers déclarés ou raccordés.
- 0 dossier de règle native sans `pub mod`.
- 0 dossier de règle native sans `register()`.
- 0 dossier de règle native sans tests locaux.

Note: `delegated` n'a pas de `register()` direct parce que c'est un module agrégateur. Il est bien raccordé via `delegated::register_all()` et `delegated::register_tsgolint()`.

### FIX: 8 marqueurs Rust Clippy n'avaient aucun test local

Règles concernées:

- `rust_arc_non_send_sync`
- `rust_await_holding_lock`
- `rust_explicit_iter_loop`
- `rust_large_enum_variant`
- `rust_no_box_default`
- `rust_no_linkedlist`
- `rust_ptr_arg`
- `rust_redundant_clone`

Ces règles sont des marqueurs délégués à Clippy. Elles ne produisent pas de diagnostic tree-sitter local, mais elles doivent quand même vérifier que leur `RuleDef` expose:

- le bon id comply;
- la bonne sévérité;
- `Language::Rust`;
- le ou les bons lints `Backend::Clippy`;
- des métadonnées exploitables.

Status: corrigé pendant la reprise. Chaque règle a maintenant deux tests locaux. Un helper partagé vérifie les bindings Clippy attendus.

Validation ciblée: `cargo test rust_` passe: 268 tests OK.

### RESULTAT FINAL: tous les tests passent

Commande finale: `cargo test`

Résultat:

- Tests unitaires: 7782 passés, 0 échec.
- `src/bin/regen-clippy-lints.rs`: 3 passés.
- `tests/e2e_cli.rs`: 8 passés.
- `tests/e2e_regressions.rs`: 6 passés.
- `tests/e2e_rules.rs`: 4 passés.

Reste seulement un warning existant et non bloquant: `src/rules/jsdoc_text_helpers.rs:12`, champ `raw` jamais lu.

## Synthèse finale

La review des règles est terminée sur tout `src/rules`.

Findings corrigés:

- Règles CSS/Dockerfile/K8s présentes sur disque mais non exposées par le registry.
- Deux règles CSS dont les tests révélaient une dépendance fragile à la forme tree-sitter.
- `k8s_no_writable_host_mount` qui laissait passer un cas dangereux.
- Couverture insuffisante des règles Kubernetes cross-manifest.
- 8 marqueurs Rust Clippy sans test local.

Etat après corrections:

- Toutes les règles natives détectées sont raccordées.
- Toutes les règles natives ont au moins deux tests locaux.
- Les tests ciblés CSS et K8s passent.
- La suite complète passe.
- Aucun blocker ouvert dans le périmètre review.

## Journal

- 2026-04-25: reprise après interruption, création de ce document.
- 2026-04-25: comparaison mécanique terminée; premier blocker identifié sur l'enregistrement incomplet des règles.
- 2026-04-25: `cargo test` initial lancé: 7607 tests OK, 2 tests CSS KO.
- 2026-04-25: correction des deux règles CSS cassées; `cargo test css_` passe.
- 2026-04-25: enregistrement complet des règles CSS, Dockerfile et Kubernetes dans `src/rules/mod.rs`; `cargo check` passe.
- 2026-04-25: correction de `k8s_no_writable_host_mount`; test ciblé OK.
- 2026-04-25: ajout de tests Kubernetes cross-manifest via `K8sIndex`; `cargo test k8s_` passe.
- 2026-04-25: `cargo test` final passe intégralement.
- 2026-04-25: reprise du périmètre sur tous les dossiers de règles sous `src/rules`.
- 2026-04-25: ajout de tests pour 8 marqueurs Rust Clippy sans couverture locale; `cargo test rust_` passe.
- 2026-04-25: `cargo test` final après reprise passe intégralement: 7782 unitaires, 3 bin, 8/6/4 e2e.
- 2026-04-26: nettoyage du bruit de formatage accidentel; contrôle mécanique complet toujours OK.
- 2026-04-26: `cargo test` relancé après nettoyage: 7782 unitaires, 3 bin, 8/6/4 e2e OK.
