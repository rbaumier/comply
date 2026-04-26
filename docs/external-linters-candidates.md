# Linters externes — Règles candidates pour comply

Généré le : 2026-04-25
Sources : hadolint, kube-linter, stylelint, csslint

Ce document inventorie chaque règle de quatre linters externes et les classe en **Recommandée**, **Peut-être / Basse priorité**, ou **Passer** pour intégration dans comply. Les règles déjà couvertes par des règles comply existantes sont listées sous **Passer** avec la règle couvrante référencée.

## Tableau récapitulatif

| Domaine       | Source(s)              | Total règles | Recommandées | Peut-être | Passer (déjà couverte / formatting / obsolète / niche) |
|---------------|------------------------|--------------|--------------|-----------|--------------------------------------------------------|
| Dockerfile    | hadolint               | 64           | 27           | 9         | 28                                                     |
| Kubernetes    | kube-linter            | 63           | 25           | 14        | 24                                                     |
| CSS           | stylelint + csslint    | 186          | 31           | 18        | 137                                                    |
| **Total**     |                        | **313**      | **83**       | **41**    | **189**                                                |

---

## Dockerfile (source : hadolint)

### Recommandées

| ID comply proposé                           | ID hadolint | Description                                                                                  | Pourquoi recommander                                                          |
|---------------------------------------------|-------------|----------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------|
| `dockerfile-absolute-workdir`               | DL3000      | WORKDIR doit utiliser un chemin absolu.                                                      | Bug : un WORKDIR relatif est fragile et surprend les stages suivants.          |
| `dockerfile-no-shell-utils-in-run`          | DL3001      | Interdire les commandes inappropriées dans RUN (ssh, vim, top, ps, kill, ifconfig, mount…).  | Détecte l'utilisation de conteneurs comme des VMs.                            |
| `dockerfile-no-cd-in-run`                   | DL3003      | Utiliser WORKDIR au lieu de `cd` dans RUN.                                                   | Bug : `cd` ne persiste pas entre les instructions RUN.                        |
| `dockerfile-no-sudo`                        | DL3004      | Interdire `sudo` dans RUN.                                                                   | Sécurité/correctness : les conteneurs ne devraient pas avoir besoin de sudo.  |
| `dockerfile-apt-clean-lists`                | DL3009      | Supprimer `/var/lib/apt/lists/*` après `apt-get install`.                                    | Bonne pratique taille d'image, oubli très fréquent.                           |
| `dockerfile-no-add-for-files`               | DL3020      | Utiliser COPY au lieu de ADD pour les fichiers/dossiers locaux.                               | ADD a des sémantiques surprenantes (fetch distant, extraction tar).            |
| `dockerfile-add-for-archive-extract`        | DL3010      | Utiliser ADD uniquement pour extraire des archives.                                           | Complète la règle précédente pour clarifier la sémantique de ADD.             |
| `dockerfile-valid-port`                     | DL3011      | Le port EXPOSE doit être entre 0 et 65535.                                                   | Bug : des typos comme `EXPOSE 80800` échouent silencieusement.                |
| `dockerfile-single-healthcheck`             | DL3012      | Interdire plusieurs instructions HEALTHCHECK.                                                | Bug : seule la dernière HEALTHCHECK prend effet.                              |
| `dockerfile-apt-get-y-flag`                 | DL3014      | `apt-get install` nécessite `-y`.                                                            | Bug : le build bloque / échoue sans `-y`.                                     |
| `dockerfile-apt-no-recommends`              | DL3015      | `apt-get install --no-install-recommends` pour des images plus légères.                      | Bonne pratique taille d'image.                                                |
| `dockerfile-apk-no-cache`                   | DL3019      | `apk add --no-cache` pour éviter `apk update` + `rm /var/cache`.                            | Bonne pratique taille d'image.                                                |
| `dockerfile-copy-trailing-slash`            | DL3021      | COPY avec >2 args doit se terminer par `/`.                                                  | Bug : écrasement silencieux du dernier argument.                              |
| `dockerfile-copy-from-known-stage`          | DL3022      | COPY --from doit référencer un alias FROM défini.                                            | Bug : une typo dans le nom du stage donne une copie vide.                     |
| `dockerfile-copy-from-not-self`             | DL3023      | COPY --from ne peut pas référencer son propre stage.                                         | Bug : construction invalide.                                                  |
| `dockerfile-unique-stage-names`             | DL3024      | Les alias de stage FROM doivent être uniques.                                                | Bug : des alias masqués produisent des COPY --from ambigus.                   |
| `dockerfile-no-apt-end-user`                | DL3027      | Utiliser `apt-get`/`apt-cache`, pas `apt`.                                                   | L'UI interactive de `apt` n'est pas stable pour les scripts.                  |
| `dockerfile-no-from-platform`               | DL3029      | Éviter `--platform` sur FROM.                                                                | Les builds cross-platform devraient utiliser le build arg `TARGETPLATFORM`.   |
| `dockerfile-yum-y-flag`                     | DL3030      | `yum install -y`.                                                                            | Bug : le build bloque sans `-y`.                                              |
| `dockerfile-yum-clean-all`                  | DL3032      | `yum clean all` après yum.                                                                   | Bonne pratique taille d'image.                                                |
| `dockerfile-zypper-non-interactive`         | DL3034      | `zypper -n` / `--non-interactive`.                                                           | Bug : le build bloque.                                                        |
| `dockerfile-no-zypper-dist-upgrade`         | DL3035      | Interdire `zypper dist-upgrade`.                                                             | Churn d'image non déterministe.                                               |
| `dockerfile-zypper-clean`                   | DL3036      | `zypper clean` après install.                                                                | Bonne pratique taille d'image.                                                |
| `dockerfile-dnf-y-flag`                     | DL3038      | `dnf install -y`.                                                                            | Bug : le build bloque.                                                        |
| `dockerfile-dnf-clean-all`                  | DL3040      | `dnf clean all` après dnf.                                                                   | Bonne pratique taille d'image.                                                |
| `dockerfile-pip-no-cache-dir`               | DL3042      | `pip install --no-cache-dir`.                                                                | Bonne pratique taille d'image.                                                |
| `dockerfile-no-onbuild-recursion`           | DL3043      | Pas de ONBUILD/FROM/MAINTAINER à l'intérieur de ONBUILD.                                     | Bug : sémantique Docker invalide.                                             |
| `dockerfile-env-no-self-reference`          | DL3044      | ENV ne peut pas référencer une variable définie dans la même instruction.                    | Bug : problème d'ordonnancement subtil qui échoue silencieusement.            |
| `dockerfile-copy-needs-workdir`             | DL3045      | COPY vers un chemin relatif nécessite un WORKDIR.                                            | Bug : la destination relative est par défaut `/`.                             |
| `dockerfile-no-multiple-cmd`                | DL4003      | Interdire plusieurs CMD.                                                                     | Bug : seul le dernier CMD est pris en compte, confusion fréquente.            |
| `dockerfile-no-multiple-entrypoint`         | DL4004      | Interdire plusieurs ENTRYPOINT.                                                              | Idem.                                                                         |
| `dockerfile-pipefail`                       | DL4006      | `SHELL ["/bin/bash", "-o", "pipefail", "-c"]` avant un RUN avec pipe.                       | Bug : les échecs dans un pipe sont silencieusement avalés.                    |
| `dockerfile-no-maintainer`                  | DL4000      | MAINTAINER est déprécié ; utiliser LABEL.                                                    | Nettoyage ; déprécié depuis 2017.                                             |
| `dockerfile-yarn-cache-clean`               | DL3060      | `yarn cache clean` après `yarn install`.                                                     | Bonne pratique taille d'image.                                                |
| `dockerfile-instruction-order`              | DL3061      | La première instruction doit être FROM, ARG ou un commentaire.                               | Bug : Dockerfile invalide.                                                    |

(35 lignes — toutes choisies car elles détectent des vrais bugs ou ont un impact majeur sur la taille d'image.)

### Peut-être / Basse priorité

| ID comply proposé                | ID hadolint   | Description                                                  | Pourquoi peut-être                                           |
| -------------------------------- | ------------- | ------------------------------------------------------------ | ------------------------------------------------------------ |
| `dockerfile-allowed-registries`  | DL3026        | Restreindre les registries FROM.                             | Utile mais intrinsèquement configurable ; nécessite une allowlist. |
| `dockerfile-shell-not-default`   | DL4005        | Utiliser SHELL pour changer le shell par défaut.             | Niche — ne concerne que les équipes utilisant `RUN ["/bin/bash", ...]`. |
| `dockerfile-no-curl-and-wget`    | DL4001        | Utiliser soit Wget soit Curl, pas les deux.                  | Règle de style défendable ; petit gain taille d'image, opinion. REVIEW: A FAIRE |
| `dockerfile-useradd-low-uid`     | DL3046        | `useradd` sans `-l` avec un UID élevé gonfle l'image.        | Bug réel mais rare (`/var/log/lastlog` sparse). REVIEW: A FAIRE |
| `dockerfile-wget-progress-flag`  | DL3047        | `wget --progress=dot` pour limiter la taille des logs.       | Propreté des logs de build ; pas critique. REVIEW: A FAIRE   |
| `dockerfile-consecutive-run`     | DL3059        | Plusieurs RUN consécutifs devraient être consolidés.         | Parfois intentionnel (cache layering) ; risque de faux positifs. |
| `dockerfile-label-not-empty`     | DL3051        | La valeur d'un LABEL ne doit pas être vide.                  | Utile quand les LABELs sont imposés par l'organisation. REVIEW: A FAIRE |
| `dockerfile-label-url-format`    | DL3052        | Un LABEL de type url doit être une URL valide.               | Idem — dépend de la politique de labels. REVIEW: A FAIRE     |
| `dockerfile-label-format-checks` | DL3053–DL3058 | Validation de format pour labels spécifiques (temps, SPDX, git hash, semver, email). | Famille de petites validations ; utile uniquement avec des labels imposés. |

### Passer

| ID hadolint         | Raison                                                                                                       |
|---------------------|--------------------------------------------------------------------------------------------------------------|
| DL1001              | Politique de pragmas inline — pas pertinent en dehors de hadolint.                                            |
| DL3002              | Déjà couvert par `dockerfile-require-non-root-user`.                                                         |
| DL3006, DL3007      | Déjà couvert par `dockerfile-no-latest-tag`.                                                                 |
| DL3008, DL3013, DL3016, DL3018, DL3028, DL3033, DL3037, DL3041, DL3062 | Déjà couvert par `dockerfile-pin-exact-version`.                               |
| DL3025              | Déjà couvert par `dockerfile-exec-form-cmd`.                                                                 |
| DL3048              | Politique configurable de validation de clés LABEL — pas du ressort d'un linter statique sans config org.    |
| DL3049              | Liste de labels requis spécifique à l'organisation.                                                          |
| DL3050              | Liste de labels superflus spécifique à l'organisation.                                                       |

---

## Kubernetes (source : kube-linter)

### Recommandées

| ID comply proposé                               | ID kube-linter                            | Description                                                                                | Pourquoi recommander                                                          |
|-------------------------------------------------|-------------------------------------------|--------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------|
| `k8s-rbac-no-create-pods`                       | access-to-create-pods                     | Sujets avec accès create sur les pods (CIS 5.1.4).                                         | Vecteur d'escalade de privilèges ; exigé par le CIS.                          |
| `k8s-rbac-no-secret-access`                     | access-to-secrets                         | Sujets avec accès get/list/watch sur les Secrets (CIS 5.1.2).                              | Violation majeure du principe du moindre privilège.                            |
| `k8s-rbac-no-cluster-admin-binding`             | cluster-admin-role-binding                | RoleBinding/ClusterRoleBinding vers `cluster-admin` (CIS 5.1.1).                           | Anti-pattern RBAC à plus fort impact.                                         |
| `k8s-no-deprecated-service-account-field`       | deprecated-service-account-field          | Pods utilisant le champ déprécié `serviceAccount`.                                          | Déprécié depuis v1.8 ; à migrer.                                             |
| `k8s-no-docker-sock-mount`                      | docker-sock                               | Un conteneur monte `docker.sock`.                                                          | Vecteur d'évasion de conteneur.                                               |
| `k8s-no-host-ipc`                               | host-ipc                                  | `hostIPC: true`.                                                                           | Rupture d'isolation des namespaces.                                           |
| `k8s-no-host-network`                           | host-network                              | `hostNetwork: true`.                                                                       | Rupture d'isolation des namespaces.                                           |
| `k8s-no-host-pid`                               | host-pid                                  | `hostPID: true`.                                                                           | Rupture d'isolation des namespaces.                                           |
| `k8s-no-privileged-container`                   | privileged-container                      | `privileged: true`.                                                                        | Flag de sécurité conteneur à plus fort impact.                                |
| `k8s-no-privileged-ports`                       | privileged-ports                          | Ports inférieurs à 1024.                                                                   | Force root ou NET_BIND_SERVICE.                                               |
| `k8s-no-sensitive-host-mounts`                  | sensitive-host-mounts                     | Montages de /, /boot, /dev, /etc, /lib, /proc, /sys, /usr.                                 | Vecteur d'évasion de conteneur.                                               |
| `k8s-no-writable-host-mount`                    | writable-host-mount                       | hostPath avec accès en écriture.                                                           | Vecteur d'évasion de conteneur.                                               |
| `k8s-no-unsafe-proc-mount`                      | unsafe-proc-mount                         | `procMount: Unmasked`.                                                                     | Affaiblit l'isolation du conteneur.                                           |
| `k8s-no-unsafe-sysctls`                         | unsafe-sysctls                            | Sysctls dangereux (kernel.msg, kernel.sem, kernel.shm, fs.mqueue., net.).                  | Affaiblit l'isolation du noyau.                                               |
| `k8s-no-allow-privileged-scc`                   | scc-deny-privileged-container             | SCC OpenShift `allowPrivilegedContainer: true`.                                            | Équivalent OpenShift du conteneur privilégié.                                 |
| `k8s-no-deprecated-extensions-api`              | no-extensions-v1beta                      | Groupe d'API `extensions/v1beta*`.                                                         | Supprimé dans K8s 1.16+ ; c'est un bug aujourd'hui.                          |
| `k8s-restart-policy-required`                   | restart-policy                            | Workload sans restartPolicy.                                                               | Bug dans les Pods standalone (les défauts peuvent être indésirables).         |
| `k8s-no-duplicate-env-vars`                     | duplicate-env-var                         | Noms de variables d'environnement dupliqués dans un conteneur.                             | Bug : shadowing silencieux.                                                   |
| `k8s-no-secret-in-env-literal`                  | env-var-secret                            | Var d'env dont le nom contient `PASSWORD`, `TOKEN`, `SECRET` avec valeur en clair.         | Détecte les secrets accidentellement en dur.                                  |
| `k8s-prefer-secret-files-over-env`              | read-secret-from-env-var                  | Secret consommé via var d'env au lieu d'un fichier monté.                                  | Bonne pratique (audit, rotation).                                             |
| `k8s-hpa-min-three-replicas`                    | hpa-minimum-three-replicas                | HPA `minReplicas < 3`.                                                                     | Bonne pratique HA (complète notre `k8s-min-replicas-two`).                    |
| `k8s-pdb-eviction-policy`                       | pdb-unhealthy-pod-eviction-policy         | PDB sans `unhealthyPodEvictionPolicy`.                                                     | Champ PDB récent (1.27+) qui prévient les rollouts bloqués.                  |
| `k8s-job-ttl-required`                          | job-ttl-seconds-after-finished            | Job sans `ttlSecondsAfterFinished`.                                                        | Propreté du cluster — oubli fréquent.                                         |
| `k8s-deployment-anti-affinity`                  | no-anti-affinity                          | Deployment multi-réplicas sans anti-affinité inter-pods.                                   | Bonne pratique HA.                                                            |
| `k8s-no-exposed-services`                       | exposed-services                          | Services de type NodePort/LoadBalancer.                                                    | Souvent involontaire ; avertir plutôt que bloquer.                            |

### Peut-être / Basse priorité

| ID comply proposé                  | ID kube-linter                                | Description                                                  | Pourquoi peut-être                                           |
| ---------------------------------- | --------------------------------------------- | ------------------------------------------------------------ | ------------------------------------------------------------ |
| `k8s-dangling-hpa`                 | dangling-horizontalpodautoscaler              | Le `scaleTargetRef` du HPA ne correspond à aucun deployment. | Nécessite un graphe YAML cross-fichiers ; coûteux.  REVIEW: A FAIRE |
| `k8s-dangling-ingress`             | dangling-ingress                              | Le backend de l'Ingress ne correspond à aucun service.       | Analyse cross-fichiers. REVIEW: A FAIRE                      |
| `k8s-dangling-network-policy`      | dangling-networkpolicy                        | Le podSelector de la NetworkPolicy ne correspond à aucun pod. | Analyse cross-fichiers. REVIEW: A FAIRE                                      |
| `k8s-dangling-network-policy-peer` | dangling-networkpolicypeer-podselector        | Le podSelector peer de la NetworkPolicy ne correspond à aucun pod. | Analyse cross-fichiers. REVIEW: A FAIRE                                      |
| `k8s-dangling-service`             | dangling-service                              | Le sélecteur du Service ne correspond à aucun pod.           | Analyse cross-fichiers.                       REVIEW: A FAIRE                |
| `k8s-dangling-service-monitor`     | dangling-servicemonitor                       | Le sélecteur du ServiceMonitor Prometheus ne correspond à aucun service. | Analyse cross-fichiers + CRD-aware. REVIEW: A FAIRE                         |
| `k8s-mismatching-selector`         | mismatching-selector                          | Le sélecteur du Deployment ne correspond pas aux labels de son template. | Bug réel, mais rare vu l'immutabilité en v1+. REVIEW: A FAIRE               |
| `k8s-non-existent-service-account` | non-existent-service-account                  | Le Pod référence un ServiceAccount inexistant.               | Analyse cross-fichiers. REVIEW: A FAIRE                                      |
| `k8s-probe-port-exists`            | liveness-port + readiness-port + startup-port | La probe cible un port que le conteneur n'expose pas.        | Vrai bug ; nécessite une résolution de nom de port. REVIEW: A FAIRE          |
| `k8s-invalid-target-ports`         | invalid-target-ports                          | Le nom du port viole le nommage IANA/K8s.                    | Niche mais facile à implémenter. REVIEW: A FAIRE                             |
| `k8s-no-ssh-port`                  | ssh-port                                      | TCP 22 exposé sur un deployment.                             | Souvent légitime (jump pods). REVIEW: A FAIRE                                |
| `k8s-env-value-from-resolves`      | env-value-from                                | `valueFrom` référence un secret/configmap absent.            | Analyse cross-fichiers. REVIEW: A FAIRE                                      |
| `k8s-priority-class-name`          | priority-class-name                           | Workload sans priorityClassName accepté.                     | Spécifique à l'organisation. REVIEW: A FAIRE                                 |
| `k8s-dnsconfig-options`            | dnsconfig-options                             | Pod sans `dnsConfig.options.ndots`.                          | Performance niche. REVIEW: A FAIRE                                           |

### Passer

| ID kube-linter                  | Raison                                                                                       |
|---------------------------------|----------------------------------------------------------------------------------------------|
| wildcard-in-rules               | Déjà couvert par `k8s-rbac-no-wildcard-resources` + `k8s-rbac-no-wildcard-verbs`.            |
| default-service-account         | Déjà couvert par `k8s-no-default-service-account`.                                           |
| latest-tag                      | Déjà couvert par `k8s-no-latest-image-tag`.                                                  |
| use-namespace                   | Déjà couvert par `k8s-require-explicit-namespace`.                                           |
| drop-net-raw-capability         | Déjà couvert par `k8s-require-drop-all-caps`.                                                |
| no-read-only-root-fs            | Déjà couvert par `k8s-require-read-only-root`.                                               |
| privilege-escalation-container  | Déjà couvert par `k8s-disallow-privilege-escalation`.                                        |
| run-as-non-root                 | Déjà couvert par `k8s-require-run-as-non-root`.                                              |
| no-liveness-probe               | Déjà couvert par `k8s-require-liveness-probe`.                                               |
| no-readiness-probe              | Déjà couvert par `k8s-require-readiness-probe`.                                              |
| unset-cpu-requirements          | Déjà couvert par `k8s-require-resource-limits` + `k8s-require-resource-requests`.            |
| unset-memory-requirements       | Idem.                                                                                        |
| minimum-three-replicas          | Partiellement couvert par `k8s-min-replicas-two` ; question de seuil.                        |
| pdb-max-unavailable             | Partiellement couvert par `k8s-require-pod-disruption-budget`.                               |
| pdb-min-available               | Partiellement couvert par `k8s-require-pod-disruption-budget`.                               |
| non-isolated-pod                | Déjà couvert par `k8s-require-network-policy`.                                               |
| no-rolling-update-strategy      | Partiellement couvert par `k8s-rolling-update-zero-unavailable`.                             |
| schema-validation               | Hors périmètre : intégration kubeconform, pas une règle de linter statique.                  |
| sorted-keys                     | Pur formatting ; géré par yamlfmt/prettier.                                                  |
| required-annotation-email       | Liste configurable spécifique à l'organisation.                                              |
| required-label-owner            | Liste configurable spécifique à l'organisation.                                              |
| no-node-affinity                | Opinion forte qui ne se généralise pas ; beaucoup de workloads n'ont intentionnellement pas de nodeAffinity. |

---

## CSS (sources : stylelint, csslint)

### Recommandées

| ID comply proposé                                | Règle source                                      | Description                                                                                | Pourquoi recommander                                                          |
|--------------------------------------------------|---------------------------------------------------|--------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------|
| `css-no-invalid-hex`                             | stylelint: color-no-invalid-hex                   | Interdire les couleurs hex invalides.                                                      | Bug : couleur silencieusement cassée.                                         |
| `css-no-empty-block`                             | stylelint: block-no-empty                         | Interdire les blocs `{}` vides.                                                            | Nettoyage + code mort.                                                        |
| `css-no-empty-comment`                           | stylelint: comment-no-empty                       | Interdire les commentaires vides.                                                          | Nettoyage ; pas cher.                                                         |
| `css-no-redundant-longhand`                      | stylelint: declaration-block-no-redundant-longhand-properties | Utiliser le shorthand au lieu d'une séquence de longhands.                   | Nettoyage ; bonne pratique CSS.                                               |
| `css-no-shorthand-overrides-longhand`            | stylelint: declaration-block-no-shorthand-property-overrides | Le shorthand réinitialise silencieusement un longhand précédent.             | Bug : piège CSS classique.                                                    |
| `css-no-duplicate-properties`                    | stylelint: declaration-block-no-duplicate-properties | Propriété dupliquée dans un même bloc.                                                 | Bug : déclaration morte.                                                      |
| `css-no-duplicate-custom-properties`             | stylelint: declaration-block-no-duplicate-custom-properties | `--foo` dupliqué dans un bloc.                                                 | Bug.                                                                          |
| `css-no-deprecated-property-value`               | stylelint: declaration-property-value-keyword-no-deprecated | Valeurs de mots-clés dépréciées.                                              | Détecte les valeurs CSS retirées.                                             |
| `css-no-unknown-property-value`                  | stylelint: declaration-property-value-no-unknown  | Valeurs inconnues pour des propriétés connues.                                             | Bug.                                                                          |
| `css-custom-property-needs-var`                  | stylelint: custom-property-no-missing-var-function | Utilisation d'un nom de custom-property sans `var()`.                                     | Bug : `--brand` au lieu de `var(--brand)` est silencieusement ignoré.         |
| `css-font-family-quotes`                         | stylelint: font-family-name-quotes                | Les noms de police multi-mots doivent être entre guillemets.                               | Bug au parsing.                                                               |
| `css-no-duplicate-font-family`                   | stylelint: font-family-no-duplicate-names         | Noms dupliqués dans la pile font-family.                                                   | Bug ; typo fréquente.                                                         |
| `css-font-family-needs-generic`                  | stylelint: font-family-no-missing-generic-family-keyword | font-family doit se terminer par une famille générique.                       | UX : pas de fallback si la police ne charge pas.                              |
| `css-calc-needs-spaces`                          | stylelint: function-calc-no-unspaced-operator     | Les opérateurs de calc nécessitent des espaces.                                            | Bug : `calc(100%-10px)` est invalide.                                         |
| `css-no-unknown-function`                        | stylelint: function-no-unknown                    | Fonctions CSS inconnues.                                                                   | Bug.                                                                          |
| `css-no-nonstandard-gradient-direction`          | stylelint: function-linear-gradient-no-nonstandard-direction | La direction de linear-gradient doit utiliser `to <côté>`.                    | Bug : syntaxe legacy invalide dans les navigateurs modernes.                  |
| `css-keyframe-no-duplicate-selectors`            | stylelint: keyframe-block-no-duplicate-selectors  | Même sélecteur deux fois dans les keyframes.                                               | Bug : écrasement silencieux.                                                  |
| `css-keyframe-no-important`                      | stylelint: keyframe-declaration-no-important      | `!important` est invalide dans @keyframes.                                                 | Bug : invalide selon la spec.                                                 |
| `css-no-unknown-media-feature`                   | stylelint: media-feature-name-no-unknown          | Features @media inconnues.                                                                 | Bug.                                                                          |
| `css-no-unknown-media-value`                     | stylelint: media-feature-name-value-no-unknown    | Valeurs inconnues dans les media features.                                                 | Bug.                                                                          |
| `css-no-invalid-media-query`                     | stylelint: media-query-no-invalid                 | Syntaxe de media query invalide.                                                           | Bug.                                                                          |
| `css-no-deprecated-media-type`                   | stylelint: media-type-no-deprecated               | Types de média dépréciés (`tv`, `projection`…).                                           | Détecte les requêtes obsolètes.                                               |
| `css-no-invalid-grid-areas`                      | stylelint: named-grid-areas-no-invalid            | grid-template-areas invalide.                                                              | Bug ; cran d'arrêt pour les layouts grid.                                     |
| `css-no-duplicate-import`                        | stylelint: no-duplicate-at-import-rules           | `@import` dupliqué.                                                                        | Nettoyage + perf.                                                             |
| `css-no-double-slash-comments`                   | stylelint: no-invalid-double-slash-comments       | Les commentaires `//` sont invalides en CSS.                                               | Bug ; fréquent en copiant du JS/SCSS.                                         |
| `css-import-position`                            | stylelint: no-invalid-position-at-import-rule     | `@import` doit être en haut du fichier.                                                    | Bug : ignoré silencieusement sinon.                                           |
| `css-no-irregular-whitespace`                    | stylelint: no-irregular-whitespace                | Caractères zero-width / NBSP dans le CSS.                                                  | Bug ; erreur de parsing difficile à débugger.                                 |
| `css-no-unknown-animation-name`                  | stylelint: no-unknown-animations                  | `animation-name` référence un @keyframes non défini.                                       | Bug ; casse fréquente lors de refactorings.                                   |
| `css-no-unknown-custom-properties`               | stylelint: no-unknown-custom-properties           | Custom property utilisée sans être définie.                                                | Bug.                                                                          |
| `css-no-deprecated-property`                     | stylelint: property-no-deprecated                 | Propriétés CSS dépréciées (`clip`, `azimuth`…).                                           | Détecte du CSS en cul-de-sac.                                                 |
| `css-no-unknown-property`                        | stylelint: property-no-unknown                    | Noms de propriétés inconnus.                                                               | Bug ; détecte les typos comme `colour`.                                       |
| `css-string-no-newline`                          | stylelint: string-no-newline                      | Sauts de ligne non échappés dans les chaînes.                                              | Bug : erreur de parsing.                                                      |
| `css-no-unknown-unit`                            | stylelint: unit-no-unknown                        | Unités inconnues comme la typo `pxx`.                                                      | Bug.                                                                          |
| `css-no-unknown-pseudo-class`                    | stylelint: selector-pseudo-class-no-unknown       | Pseudo-classes inconnues.                                                                  | Bug ; détecte les typos comme `:hove`.                                        |
| `css-no-unknown-pseudo-element`                  | stylelint: selector-pseudo-element-no-unknown     | Pseudo-éléments inconnus.                                                                  | Bug.                                                                          |
| `css-no-unknown-type-selector`                   | stylelint: selector-type-no-unknown               | Sélecteurs d'éléments HTML inconnus.                                                       | Bug ; détecte `dvi` au lieu de `div`.                                         |
| `css-no-deprecated-selector`                     | stylelint: selector-no-deprecated                 | Sélecteurs dépréciés (ex: `:-webkit-any`).                                                 | Nettoyage.                                                                    |
| `css-no-unmatchable-selector`                    | stylelint: selector-anb-no-unmatchable            | `:nth-child(0)` et An+B similaires qui ne matchent rien.                                  | Bug.                                                                          |
| `css-no-redundant-shorthand-values`              | stylelint: shorthand-property-no-redundant-values | `margin: 10px 10px 10px 10px` se simplifie en `margin: 10px`.                             | Nettoyage.                                                                    |
| `css-no-zero-units`                              | stylelint: length-zero-no-unit                    | `0px` devrait être `0`.                                                                    | Nettoyage.                                                                    |
| `css-no-redundant-nesting`                       | stylelint: block-no-redundant-nested-style-rules  | Nesting redondant qui n'ajoute que du bruit.                                               | Nettoyage.                                                                    |
| `css-no-descending-specificity`                  | stylelint: no-descending-specificity              | Règle de spécificité basse après une de spécificité haute.                                 | Bug : surprises d'ordonnancement de la cascade.                               |
| `css-rgba-fallback`                              | csslint: fallback-colors                          | Fournir un fallback opaque avant RGBA/HSLA.                                                | Résilience pour les renderers anciens ; rare mais pas cher.                   |

### Peut-être / Basse priorité

| ID comply proposé                             | Règle source                                      | Description                                                                                | Pourquoi peut-être                                                         |
|-----------------------------------------------|---------------------------------------------------|--------------------------------------------------------------------------------------------|----------------------------------------------------------------------------|
| `css-no-empty-source`                         | stylelint: no-empty-source                        | Interdire les fichiers CSS vides.                                                          | Nettoyage ; rare.                                                          |
| `css-no-vendor-prefix-property`               | stylelint: property-no-vendor-prefix              | Interdire `-webkit-` etc.                                                                  | Utile mais interagit avec autoprefixer. REVIEW: A FAIRE                                    |
| `css-no-vendor-prefix-selector`               | stylelint: selector-no-vendor-prefix              | Interdire les pseudo-éléments vendor.                                                      | Idem. REVIEW: A FAIRE                                                                      |
| `css-no-vendor-prefix-value`                  | stylelint: value-no-vendor-prefix                 | Interdire les valeurs `-webkit-`.                                                          | Idem. REVIEW: A FAIRE                                                                      |
| `css-no-vendor-prefix-media`                  | stylelint: media-feature-name-no-vendor-prefix    | Interdire les media features vendor-prefixed.                                              | Idem. REVIEW: A FAIRE                                                                      |
| `css-no-vendor-prefix-at-rule`                | stylelint: at-rule-no-vendor-prefix               | Interdire les at-rules vendor.                                                             | Idem. REVIEW: A FAIRE                                                                      |
| `css-no-deprecated-at-rule`                   | stylelint: at-rule-no-deprecated                  | At-rules dépréciées.                                                                       | Niche. REVIEW: A FAIRE                                                                     |
| `css-no-unknown-at-rule`                      | stylelint: at-rule-no-unknown                     | At-rules inconnues.                                                                        | Utile mais entre en conflit avec @tailwind, @apply, @scope, @layer.        |
| `css-at-rule-prelude-valid`                   | stylelint: at-rule-prelude-no-invalid             | Prélude invalide sur les at-rules.                                                         | Utile mais lourd côté parser.                                              |
| `css-id-selector-overuse`                     | csslint: ids                                      | Éviter `#id` dans les sélecteurs.                                                          | Avis tranché ; dépend du style de l'équipe.                                |
| `css-no-import`                               | csslint: import                                   | Préférer `<link>` à `@import`.                                                             | Opinion performance ; dépend du bundler.                                   |
| `css-overqualified-element`                   | csslint: overqualified-elements                   | `a.btn` est redondant.                                                                     | Style ; parfois intentionnel.                                              |
| `css-qualified-headings`                      | csslint: qualified-headings                       | Ne pas qualifier `h1..h6`.                                                                 | Style ; gains mineurs.                                                     |
| `css-unique-headings`                         | csslint: unique-headings                          | Règles de titres définies plus d'une fois.                                                 | Parfois intentionnel.                                                      |
| `css-no-regex-selector`                       | csslint: regex-selectors                          | `[class^=]`, `[id$=]` sont plus lents.                                                    | Les moteurs modernes gèrent bien ; signal faible.                          |
| `css-no-universal-selector`                   | csslint: universal-selector                       | `*` est lent.                                                                              | Souvent intentionnel (resets) ; signal faible en 2026.                     |
| `css-no-adjoining-classes`                    | csslint: adjoining-classes                        | `.foo.bar` pour legacy IE.                                                                 | Les navigateurs modernes gèrent ; surtout legacy.                          |
| `css-no-display-property-grouping`            | csslint: display-property-grouping                | Ex : `display: inline` avec `width`.                                                       | Détecte des contradictions ; moyennement fréquent.                         |
| `css-outline-none-needs-focus`                | csslint: outline-none                             | `outline: none` devrait être limité aux règles `:focus`.                                   | Accessibilité ; pourrait être utile. REVIEW: A FAIRE                                       |
| `css-text-indent-bounds`                      | csslint: text-indent                              | `text-indent: -9999px` est une vieille astuce de masquage.                                 | Technique ancienne ; rarement vue.                                         |

### Passer

| Règle source                                                                                     | Raison                                                                       |
|--------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------|
| stylelint: alpha-value-notation                                                                  | Pur formatting.                                                              |
| stylelint: color-function-alias-notation                                                         | Pur formatting.                                                              |
| stylelint: color-function-notation                                                               | Pur formatting.                                                              |
| stylelint: color-hex-alpha                                                                       | Pur formatting (configurable).                                               |
| stylelint: color-hex-length                                                                      | Pur formatting.                                                              |
| stylelint: color-named                                                                           | Pur formatting (configurable).                                               |
| stylelint: color-no-hex                                                                          | Pur formatting (configurable).                                               |
| stylelint: at-rule-empty-line-before                                                             | Pur formatting.                                                              |
| stylelint: comment-empty-line-before                                                             | Pur formatting.                                                              |
| stylelint: comment-pattern                                                                       | Pur formatting (regex configurable).                                         |
| stylelint: comment-whitespace-inside                                                             | Pur formatting.                                                              |
| stylelint: comment-word-disallowed-list                                                          | Liste configurable.                                                          |
| stylelint: container-name-pattern                                                                | Pur formatting.                                                              |
| stylelint: custom-media-pattern                                                                  | Pur formatting.                                                              |
| stylelint: custom-property-empty-line-before                                                     | Pur formatting.                                                              |
| stylelint: custom-property-pattern                                                               | Pur formatting.                                                              |
| stylelint: declaration-block-single-line-max-declarations                                        | Pur formatting.                                                              |
| stylelint: declaration-empty-line-before                                                         | Pur formatting.                                                              |
| stylelint: declaration-property-max-values                                                       | Seuil configurable.                                                          |
| stylelint: declaration-property-unit-allowed-list                                                | Allow-list configurable.                                                     |
| stylelint: declaration-property-unit-disallowed-list                                             | Configurable.                                                                |
| stylelint: declaration-property-value-allowed-list                                               | Configurable.                                                                |
| stylelint: declaration-property-value-disallowed-list                                            | Configurable.                                                                |
| stylelint: declaration-no-important                                                              | Avis tranché ; interdire `!important` partout est trop strict pour un linter générique. |
| stylelint: display-notation                                                                      | Pur formatting.                                                              |
| stylelint: font-weight-notation                                                                  | Pur formatting.                                                              |
| stylelint: function-name-case                                                                    | Pur formatting.                                                              |
| stylelint: function-url-quotes                                                                   | Pur formatting.                                                              |
| stylelint: function-url-no-scheme-relative                                                       | Préoccupation sécurité niche, configurable.                                  |
| stylelint: function-url-scheme-allowed-list / disallowed-list                                    | Liste configurable.                                                          |
| stylelint: function-allowed-list / disallowed-list                                               | Liste configurable.                                                          |
| stylelint: hue-degree-notation                                                                   | Pur formatting.                                                              |
| stylelint: import-notation                                                                       | Pur formatting.                                                              |
| stylelint: keyframe-selector-notation                                                            | Pur formatting.                                                              |
| stylelint: keyframes-name-pattern                                                                | Pur formatting.                                                              |
| stylelint: layer-name-pattern                                                                    | Pur formatting.                                                              |
| stylelint: lightness-notation                                                                    | Pur formatting.                                                              |
| stylelint: max-nesting-depth                                                                     | Seuil configurable.                                                          |
| stylelint: media-feature-name-allowed-list / disallowed-list                                     | Liste configurable.                                                          |
| stylelint: media-feature-name-unit-allowed-list                                                  | Liste configurable.                                                          |
| stylelint: media-feature-name-value-allowed-list                                                 | Liste configurable.                                                          |
| stylelint: media-feature-range-notation                                                          | Pur formatting.                                                              |
| stylelint: at-rule-allowed-list / disallowed-list                                                | Liste configurable.                                                          |
| stylelint: at-rule-property-required-list                                                        | Liste configurable.                                                          |
| stylelint: at-rule-descriptor-no-unknown                                                         | Niche ; check récent.                                                        |
| stylelint: at-rule-descriptor-value-no-unknown                                                   | Niche.                                                                       |
| stylelint: annotation-no-unknown                                                                 | Niche ; annotations SCSS uniquement.                                         |
| stylelint: nesting-selector-no-missing-scoping-root                                              | Niche ; CSS Nesting bleeding edge.                                           |
| stylelint: no-duplicate-selectors                                                                | Parfois intentionnel (overrides de thème) ; sujet aux faux positifs.         |
| stylelint: no-invalid-position-declaration                                                       | Niche ; règle positionnelle récente.                                         |
| stylelint: number-max-precision                                                                  | Pur formatting.                                                              |
| stylelint: property-allowed-list / disallowed-list                                               | Liste configurable.                                                          |
| stylelint: property-layout-mappings                                                              | Partiellement couvert par `i18n-prefer-logical-css-properties`.              |
| stylelint: relative-selector-nesting-notation                                                    | Pur formatting.                                                              |
| stylelint: rule-empty-line-before                                                                | Pur formatting.                                                              |
| stylelint: rule-nesting-at-rule-required-list                                                    | Liste configurable.                                                          |
| stylelint: rule-selector-property-disallowed-list                                                | Liste configurable.                                                          |
| stylelint: selector-attribute-name-disallowed-list                                               | Configurable.                                                                |
| stylelint: selector-attribute-operator-allowed-list / disallowed-list                            | Configurable.                                                                |
| stylelint: selector-attribute-quotes                                                             | Pur formatting.                                                              |
| stylelint: selector-class-pattern                                                                | Pur formatting (BEM etc.).                                                   |
| stylelint: selector-combinator-allowed-list / disallowed-list                                    | Configurable.                                                                |
| stylelint: selector-disallowed-list                                                              | Configurable.                                                                |
| stylelint: selector-id-pattern                                                                   | Pur formatting.                                                              |
| stylelint: selector-max-attribute / class / combinators / compound-selectors / id / pseudo-class / specificity / type / universal | Seuils configurables.                                                        |
| stylelint: selector-nested-pattern                                                               | Pur formatting.                                                              |
| stylelint: selector-no-qualifying-type                                                           | Opinion de style (csslint couvre pareil).                                     |
| stylelint: selector-not-notation                                                                 | Pur formatting.                                                              |
| stylelint: selector-pseudo-class-allowed-list / disallowed-list                                  | Configurable.                                                                |
| stylelint: selector-pseudo-element-allowed-list / disallowed-list                                | Configurable.                                                                |
| stylelint: selector-pseudo-element-colon-notation                                                | Pur formatting.                                                              |
| stylelint: selector-type-case                                                                    | Pur formatting.                                                              |
| stylelint: syntax-string-no-invalid                                                              | Niche ; uniquement pour CSS-in-JS déclaratif.                                |
| stylelint: time-min-milliseconds                                                                 | Seuil configurable.                                                          |
| stylelint: unit-allowed-list / disallowed-list                                                   | Liste configurable.                                                          |
| stylelint: unit-no-unknown                                                                       | Déjà dans Recommandées sous `css-no-unknown-unit`.                           |
| stylelint: value-keyword-case                                                                    | Pur formatting.                                                              |
| stylelint: no-unknown-custom-media                                                               | Déjà dans Recommandées (classe de bug custom-media).                         |
| csslint: box-model                                                                               | `box-sizing: border-box` par défaut rend cette règle obsolète.               |
| csslint: box-sizing                                                                              | Cible IE6/7 — obsolète.                                                      |
| csslint: bulletproof-font-face                                                                   | Cible les anciens IE — obsolète.                                             |
| csslint: compatible-vendor-prefixes                                                              | Territoire d'autoprefixer.                                                   |
| csslint: duplicate-background-images                                                             | Configurable ; les bundlers gèrent.                                          |
| csslint: duplicate-properties                                                                    | Déjà dans Recommandées via l'équivalent stylelint.                           |
| csslint: empty-rules                                                                             | Déjà dans Recommandées via l'équivalent stylelint (`block-no-empty`).        |
| csslint: errors                                                                                  | Erreurs de parser — couvertes implicitement.                                 |
| csslint: floats                                                                                  | Seuil configurable.                                                          |
| csslint: font-faces                                                                              | Seuil configurable.                                                          |
| csslint: font-sizes                                                                              | Seuil configurable.                                                          |
| csslint: gradients                                                                               | Territoire d'autoprefixer.                                                   |
| csslint: import-ie-limit                                                                         | Cible IE — obsolète.                                                         |
| csslint: important                                                                               | Même avis que stylelint declaration-no-important ; passé.                    |
| csslint: known-properties                                                                        | Déjà dans Recommandées (`css-no-unknown-property`).                          |
| csslint: order-alphabetical                                                                      | Pur formatting.                                                              |
| csslint: performant-transitions                                                                  | Déjà couvert par le travail perf adjacent `perf-prefers-reduced-motion`.     |
| csslint: rules-count                                                                             | Métrique, pas un lint.                                                       |
| csslint: selector-max / selector-max-approaching                                                 | Cible la limite IE de 4095 règles — obsolète.                                |
| csslint: selector-newline                                                                        | Pur formatting.                                                              |
| csslint: shorthand                                                                               | Déjà dans Recommandées via stylelint `declaration-block-no-redundant-longhand-properties`. |
| csslint: star-property-hack                                                                      | Cible IE6/7 — obsolète.                                                      |
| csslint: underscore-property-hack                                                                | Cible IE6 — obsolète.                                                        |
| csslint: unqualified-attributes                                                                  | Les moteurs modernes ne sont pas affectés.                                   |
| csslint: vendor-prefix                                                                           | Territoire d'autoprefixer.                                                   |
| csslint: zero-units                                                                              | Déjà dans Recommandées (`css-no-zero-units`).                                |

---

## Notes sur les comptages

La colonne Passer regroupe trois sous-raisons distinctes :

1. **Déjà couverte** par une règle comply existante (l'exemple principal dans chaque domaine est documenté en ligne ci-dessus).
2. **Pur formatting / liste configurable** — hors périmètre de comply ; prettier/stylelint config gère ça.
3. **Obsolète** — checks de l'ère IE6/7 qui ne correspondent plus au baseline navigateur de 2026.

Quand une règle stylelint/csslint chevauche une autre dans le même domaine, la place Recommandée liste le nom canonique ; le doublon est classé sous Passer avec une référence retour.
