Voici une classification structurée des règles déclenchées par ton linter. Je les ai divisées en 4 catégories : **Bugs critiques (AST/Logique)**, **Faux positifs de contexte (React/Vite)**, **Règles trop strictes (Bruit)**, et **Excellentes règles (Pertinentes)**.

### 🐛 1. Bugs d'analyse AST & Logique (À corriger en priorité)
Ces règles se déclenchent de manière aberrante car ton outil d'analyse statique identifie mal les nœuds de l'AST (Abstract Syntax Tree) ou applique un contexte au mauvais endroit.

| Règle | Statut | Message du linter | Extrait de code concerné | Interprétation & Conseil |
| :--- | :--- | :--- | :--- | :--- |
| `[regex-*]` *(toutes les règles regex)* | **BUG CRITIQUE** | *Duplicate character in regex character class* | `plugins: [react(), tailwindcss()...]`<br>*(vite.config.ts)* | Ton parseur lit **des chaînes de caractères standards ou du code** comme des expressions régulières. Assure-toi que le visiteur AST ne cible que les nœuds de type `RegExpLiteral` ou `new RegExp()`. |
| `[sql-no-timestamp...]` | **BUG CRITIQUE** | *`TIMESTAMP` without timezone — use `TIMESTAMPTZ`.* | `if (type.includes("timestamp"))`<br>*(filter-chip.tsx)* | Ton linter analyse de simples chaînes JavaScript (String literals) en y cherchant des failles SQL. Il doit se restreindre aux templates SQL (ex: `` sql`...` ``) ou aux fichiers `.sql`. |
| `[tailwind-no-conflicting-classes]` | **BUG LOGIQUE** | *Conflicting `text-` classes: text-xs, text-muted-foreground* | `className="text-muted-foreground text-xs"`<br>*(filter-chip.tsx)* | `text-xs` (taille de police) et `text-muted-foreground` (couleur) ne rentrent pas en conflit. Ton linter suppose à tort que toutes les classes commençant par `text-` font la même chose. |
| `[no-unthrown-error]` | **BUG AST** | *`new Error(...)` is created but never thrown* | `throw new Error(body.detail ?? ...)`<br>*(use-query-data.ts)* | L'erreur est bien jetée ! Ton parseur détecte l'instanciation de `new Error`, mais ne remonte pas à son nœud parent pour voir qu'il est rattaché à un mot-clé `throw`. |
| `[generator-without-yield]` | **BUG AST** | *Generator function does not contain a `yield`* | `onClick={() => toggle(name)}`<br>*(aggregate-mode.tsx)* | Détecte une simple fonction fléchée comme un générateur. Ton AST doit distinguer `ArrowFunctionExpression` de `FunctionDeclaration` avec la propriété `generator: true`. |
| `[ts-no-invalid-void-type]` | **BUG LOGIQUE** | *`void` is only valid as a return type* | `onChange: (value: FilterValue) => void;`<br>*(filter-chip.tsx)* | TypeScript autorise tout à fait `void` comme type de retour d'une fonction passée en prop. La règle se contredit elle-même par rapport au code. |

---

### ⚛️ 2. Faux Positifs liés au Contexte (Écosystème React / Vite)
Ces règles ont du sens pour du Node.js pur ou des librairies agnostiques, mais ne comprennent pas comment fonctionne le frontend moderne.

| Règle | Statut | Message du linter | Extrait de code concerné | Interprétation & Conseil |
| :--- | :--- | :--- | :--- | :--- |
| `[import-*]` *(default/named export)* | **Faux Positif (Vite/UI)** | *Prefer named exports / Named exports are not allowed* | `export default defineConfig(...)`<br>`export { Alert }` | La règle se contredit sur les exports (nommés vs par défaut). Surtout, Vite **exige** un `export default` dans son fichier de config. À désactiver sur les fichiers `.config.ts` et à stabiliser. |
| `[no-null]` | **Faux Positif (React)** | *Use `undefined` instead of `null`.* | `state = { error: null as Error \| null };`<br>*(main.tsx)* | En React, `null` est très utilisé sémantiquement (ne rien rendre dans l'arbre DOM, état initial d'une ErrorBoundary, d'une ref). C'est une mauvaise idée de l'interdire dans des fichiers `.tsx`. |
| `[no-class-inheritance]` | **Faux Positif (React)** | *Class inheritance via `extends` — prefer composition* | `class ErrorBoundary extends Component`<br>*(main.tsx)* | C'est l'**unique** façon de créer un ErrorBoundary en React. Ce framework impose cet héritage. Il faut ignorer cette règle si la classe s'étend de `React.Component`. |
| `[a11y-click-events-have-key-events]`| **Faux Positif (Radix/BaseUI)** | *Element has `onClick` without a corresponding keyboard* | `<DropdownMenuCheckboxItem onClick={...}>`<br>*(column-visibility.tsx)* | C'est techniquement exact, mais ici l'élément est un composant de librairie tierce (Base UI / Radix) qui **gère déjà** les événements clavier en interne. Difficile à corriger, mais crée beaucoup de bruit. |
| `[react-jsx-no-bind]` | **Obsolète** | *Arrow function in JSX prop creates a new function* | `onClick={() => toggle(name)}`<br>*(column-visibility.tsx)* | Dans les versions modernes de React, l'impact des fonctions inline sur les éléments natifs (`<button>`, `<div>`) est négligeable. À ne garder que pour les composants lourds mémorisés (`memo`). |

---

### 📢 3. Règles trop strictes (Bruit et Boilerplate)
Ces règles sont académiques mais vont rendre les développeurs fous. Les seuils sont beaucoup trop bas.

| Règle | Statut | Message du linter | Extrait de code concerné | Interprétation & Conseil |
| :--- | :--- | :--- | :--- | :--- |
| `[no-magic-numbers]` | **Trop strict** | *No magic number: 0* | `alignOffset = 0`<br>*(dropdown-menu.tsx)* | Traiter `0`, `1`, ou `-1` comme des nombres magiques est insupportable (ex: accès au tableau `length - 1`). Exclut ces chiffres, et ignore idéalement les composants d'UI où l'on gère des pixels. |
| `[jsdoc-*]` / `[module-header]` | **Bruit massif** | *Exported function '...' is missing a JSDoc block.* | `export function FilterChip(...)`<br>*(filter-chip.tsx)* | Imposer de la JSDoc sur 100% des composants React internes est inutile. C'est du bruit. Règle à réserver aux fonctions exportées d'un dossier `/lib` ou `utils.ts` uniquement. |
| `[id-length]` | **Trop strict** | *Identifier name is too short (< 2).* | `onChange={(v) => ...}`<br>*(aggregate-mode.tsx)* | Dans une lambda fonction courte, `v` (value), `e` (event), ou `i` (index) sont des conventions ultra-lisibles. La règle devrait ignorer les fonctions inline. |
| `[max-file-lines]` / `[max-function-lines]`| **Seuils irréalistes** | *Function 'DropdownMenuCheckboxItem' is 32 NCLOC (max 30)* | `function DropdownMenuCheckboxItem(...)`<br>*(dropdown-menu.tsx)* | 30 lignes max pour une fonction en React (JSX compris), c'est beaucoup trop court. Pousse ce seuil à 100-150 lignes pour du React, et 250-300 pour un fichier. |
| `[colocated-tests]` | **Dogmatique** | *No colocated test file found for `alert.tsx`* | `alert.tsx` (Composant d'UI basique) | Imposer un test pour chaque composant visuel primitif généré (type Shadcn) a très peu de valeur métier. |

---

### 🏆 4. Excellentes règles (Celles qui brillent vraiment !)
C'est ici que ton linter montre tout son potentiel et apporte une vraie valeur architecturale.

| Règle | Statut | Message du linter | Extrait de code concerné | Interprétation & Conseil |
| :--- | :--- | :--- | :--- | :--- |
| `[react-no-object-in-dep-array]`| **Excellente** | *`aggregateConfig` in `useMemo` dep array — if this is an object...* | `[endpoint, aggregateConfig]`<br>*(query-table.tsx)* | **Un des meilleurs diagnostics du lot**. Placer un objet dans un `[deps]` React cause souvent des boucles infinies. C'est hyper pertinent. *(Attention à ne pas flagguer les strings/booléens)*. |
| `[timeout-on-io]` | **Super Pratique** | *I/O call without a timeout — network calls can hang forever.* | `fetch(url)`<br>*(use-query-data.ts)* | Fantastique conseil de résilience. Oublier l'`AbortSignal` dans un `fetch` est une erreur ultra courante qui cause des fuites de mémoire. |
| `[no-common-grab-bag]` | **Vision d'Architècte**| *File 'utils.*' is a grab-bag name — pick a name that describes...* | `lib/utils.ts` | J'adore cette règle. Les fichiers `utils.ts` finissent toujours en décharge municipale de code. Pousser à la sémantique est top. |
| `[banned-identifiers]` | **Super Sémantique** | *Rename 'handleRun' — use intent over implementation.* | `function handleRun()`<br>*(aggregate-mode.tsx)* | Les préfixes `handle` ou les variables `data` rendent le code paresseux. Pousser à nommer par l'intention (`onFilterUpdate`, `tableData`) est une super pratique. |
| `[prefer-at]` | **Moderne** | *Prefer `.at(…)` over `[….length - index]`.* | `cursorStack[cursorStack.length - 1]`<br>*(use-cursor-pagination.ts)* | Super règle pour pousser à l'utilisation du JavaScript moderne (`ES2022`). |
| `[error-message-is-remediation]`| **Pragmatique** | *Error message "..." is too vague — describe what went wrong* | `throw new Error(\`Meta fetch failed... \`)` | Obliger le développeur à donner des messages d'erreur *actionnables* pour le débuggage. C'est très intelligent. |

### En résumé pour l'itération suivante :
1. Isole impérativement la résolution des AST pour tes règles de String, Regex et SQL.
2. Implémente la lecture des fichiers `tsconfig.json` ou une configuration de base pour que le linter ne crie pas sur des imports réels (`[no-implicit-deps]`).
3. Relève tes compteurs : les seuils de complexité (`max=10`), de lignes (`max=30`), et l'interdiction du chiffre `0` sont inadaptés au développement réel.
