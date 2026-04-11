# eslint-plugin-unicorn — Catalogue complet (146 règles)

> Référence pour l'implémentation native dans comply.
> Pour chaque règle : exemple de violation ❌, version corrigée ✅, et raison d'être.

---

## Table des matières

1. [Qualité du code](#qualité-du-code)
2. [Modernisation ES](#modernisation-es)
3. [Préférences de style](#préférences-de-style)
4. [Strings](#strings)
5. [Arrays](#arrays)
6. [DOM / Browser](#dom--browser)
7. [Modules / Imports](#modules--imports)
8. [Erreurs](#erreurs)
9. [Regex](#regex)
10. [Divers](#divers)

---

## Qualité du code

### 1. `consistent-function-scoping`
Déplace les fonctions au scope le plus haut possible quand elles ne capturent aucune variable locale.

```js
// ❌ Violation — la fonction ne capture rien du scope parent
function foo(bar) {
  function doSomething(x) { return x * 2; }
  return doSomething(bar);
}

// ✅ Correct
function doSomething(x) { return x * 2; }
function foo(bar) { return doSomething(bar); }
```
**Pourquoi :** Une fonction imbriquée inutilement donne l'impression qu'elle dépend de son scope parent. La remonter clarifie ses dépendances réelles et permet la réutilisation.

---

### 2. `no-nested-ternary`
Interdit les ternaires imbriquées.

```js
// ❌
const x = a ? b ? 1 : 2 : 3;

// ✅
let x;
if (a) {
  x = b ? 1 : 2;
} else {
  x = 3;
}
```
**Pourquoi :** Les ternaires imbriquées sont difficiles à lire et à déboguer. Elles masquent le flux de contrôle.

---

### 3. `no-lonely-if`
Interdit un `if` comme seule instruction dans un bloc `else`.

```js
// ❌
if (a) {
  // ...
} else {
  if (b) {
    // ...
  }
}

// ✅
if (a) {
  // ...
} else if (b) {
  // ...
}
```
**Pourquoi :** `else { if }` est un `else if` déguisé. La version aplatie est plus lisible et standard.

---

### 4. `no-negated-condition`
Interdit les conditions niées quand une branche `else` existe.

```js
// ❌
if (!isReady) {
  handleNotReady();
} else {
  handleReady();
}

// ✅
if (isReady) {
  handleReady();
} else {
  handleNotReady();
}
```
**Pourquoi :** Le cerveau traite plus facilement les conditions positives. Inverser les branches élimine la double négation mentale.

---

### 5. `no-negation-in-equality-check`
Interdit la négation dans les comparaisons d'égalité (la négation s'applique au mauvais opérande).

```js
// ❌ — !a === b signifie (!a) === b, pas !(a === b)
if (!x === true) { }

// ✅
if (x !== true) { }
```
**Pourquoi :** `!x === y` est presque toujours un bug — le développeur voulait `x !== y`. La précédence de `!` piège les lecteurs.

---

### 6. `no-object-as-default-parameter`
Interdit un objet littéral comme valeur par défaut de paramètre.

```js
// ❌
function foo(options = { timeout: 1000, retries: 3 }) { }

// ✅
function foo({ timeout = 1000, retries = 3 } = {}) { }
```
**Pourquoi :** Un objet par défaut est remplacé entièrement si l'appelant passe un objet partiel — les propriétés manquantes deviennent `undefined`. Le destructuring avec defaults par propriété résout le problème.

---

### 7. `no-unreadable-iife`
Interdit les IIFE illisibles (appel immédiat sur une fonction parenthésée complexe).

```js
// ❌
const value = (function() { return 42; })()(bar);

// ✅
const fn = function() { return 42; };
const value = fn()(bar);
```
**Pourquoi :** Les IIFE chaînées sont opaques. Extraire dans une variable nommée rend le flux explicite.

---

### 8. `no-this-assignment`
Interdit `const self = this`.

```js
// ❌
class Foo {
  bar() {
    const self = this;
    setTimeout(function() { self.doStuff(); }, 100);
  }
}

// ✅
class Foo {
  bar() {
    setTimeout(() => { this.doStuff(); }, 100);
  }
}
```
**Pourquoi :** Pattern pré-ES6 rendu obsolète par les arrow functions qui capturent le `this` lexical.

---

### 9. `no-static-only-class`
Interdit les classes qui n'ont que des membres statiques.

```js
// ❌
class MathUtils {
  static add(a, b) { return a + b; }
  static multiply(a, b) { return a * b; }
}

// ✅
export function add(a, b) { return a + b; }
export function multiply(a, b) { return a * b; }
```
**Pourquoi :** Une classe sans instance est un namespace déguisé. En ESM, les exports nommés remplissent ce rôle sans la cérémonie d'une classe.

---

### 10. `no-thenable`
Interdit d'avoir une propriété/méthode `then` sur un objet.

```js
// ❌
const obj = { then() { return 42; } };

// ✅
const obj = { execute() { return 42; } };
```
**Pourquoi :** Tout objet avec `.then()` est traité comme un thenable par `await` et `Promise.resolve()`. Ça provoque des comportements inattendus quand l'objet est accidentellement `await`é.

---

### 11. `no-useless-switch-case`
Interdit les `case` inutiles dans un `switch` (qui tombent dans le `default`).

```js
// ❌
switch (value) {
  case 'a':
  default:
    handleDefault();
}

// ✅
switch (value) {
  default:
    handleDefault();
}
```
**Pourquoi :** Un `case` qui fall-through dans le `default` sans code propre est du bruit — il n'ajoute aucune branche.

---

### 12. `prefer-switch`
Préfère `switch` quand il y a 3+ `else if` sur la même variable.

```js
// ❌
if (x === 'a') { doA(); }
else if (x === 'b') { doB(); }
else if (x === 'c') { doC(); }

// ✅
switch (x) {
  case 'a': doA(); break;
  case 'b': doB(); break;
  case 'c': doC(); break;
}
```
**Pourquoi :** Un `switch` rend explicite que toutes les branches testent la même variable. Plus lisible et optimisable.

---

### 13. `prefer-ternary`
Préfère une ternaire pour les `if/else` simples qui assignent la même variable.

```js
// ❌
let result;
if (condition) {
  result = 'yes';
} else {
  result = 'no';
}

// ✅
const result = condition ? 'yes' : 'no';
```
**Pourquoi :** Réduit 5 lignes en 1, permet `const` au lieu de `let`, et rend l'intention (assignation conditionnelle) explicite.

---

### 14. `prefer-logical-operator-over-ternary`
Préfère un opérateur logique quand la ternaire est redondante.

```js
// ❌
const value = foo ? foo : bar;

// ✅
const value = foo || bar;

// ❌
const value = foo ? bar : foo;

// ✅
const value = foo && bar;
```
**Pourquoi :** `x ? x : y` est exactement la sémantique de `x || y`. La forme logique est plus concise et idiomatique.

---

### 15. `prefer-math-min-max`
Préfère `Math.min()`/`Math.max()` aux ternaires de comparaison.

```js
// ❌
const clamped = value > max ? max : value;

// ✅
const clamped = Math.min(value, max);
```
**Pourquoi :** `Math.min`/`Math.max` sont auto-documentés — pas besoin de tracer mentalement la comparaison.

---

### 16. `no-accessor-recursion`
Interdit l'accès récursif à `this` dans les getters/setters.

```js
// ❌ — boucle infinie
class Foo {
  get bar() { return this.bar; }
  set bar(value) { this.bar = value; }
}
```
**Pourquoi :** Accéder à `this.bar` dans le getter de `bar` crée une récursion infinie — stack overflow garanti.

---

### 17. `isolated-functions`
Interdit l'usage de variables hors scope dans les fonctions isolées (callbacks de `setTimeout`, `setInterval`, etc.).

```js
// ❌
function setup() {
  let count = 0;
  setInterval(() => { count++; console.log(count); }, 1000);
}

// ✅
function setup() {
  const state = { count: 0 };
  setInterval(() => { state.count++; console.log(state.count); }, 1000);
}
```
**Pourquoi :** Les closures sur des variables mutables dans des callbacks asynchrones sont une source classique de bugs de timing.

---

### 18. `no-immediate-mutation`
Interdit la mutation immédiate après une assignation de variable.

```js
// ❌
const arr = [3, 1, 2];
arr.sort();

// ✅
const arr = [3, 1, 2].sort();
```
**Pourquoi :** Muter immédiatement après la déclaration montre que l'intention était de créer la valeur transformée — autant le faire en une expression.

---

## Modernisation ES

### 19. `no-for-loop`
Remplace les boucles `for` classiques par `for...of`.

```js
// ❌
for (let i = 0; i < arr.length; i++) {
  console.log(arr[i]);
}

// ✅
for (const item of arr) {
  console.log(item);
}
```
**Pourquoi :** `for...of` est plus lisible, moins sujet aux erreurs off-by-one, et fonctionne avec tout itérable.

---

### 20. `no-array-for-each`
Préfère `for...of` au lieu de `.forEach()`.

```js
// ❌
items.forEach(item => { process(item); });

// ✅
for (const item of items) { process(item); }
```
**Pourquoi :** `for...of` supporte `break`, `continue`, `await`, et `return` — `.forEach` ne supporte aucun de ces mécanismes de contrôle.

---

### 21. `no-new-array`
Interdit `new Array()`.

```js
// ❌
const arr = new Array(3);

// ✅
const arr = Array.from({ length: 3 });
```
**Pourquoi :** `new Array(3)` crée un tableau sparse de 3 éléments vides, pas `[3]`. L'API est ambiguë — `Array.from` est explicite.

---

### 22. `no-new-buffer`
Interdit `new Buffer()` (déprécié).

```js
// ❌
const buf = new Buffer('hello');

// ✅
const buf = Buffer.from('hello');
```
**Pourquoi :** `new Buffer()` est déprécié depuis Node 6 pour des raisons de sécurité (buffer non-initialisé pouvant fuiter de la mémoire).

---

### 23. `new-for-builtins`
Force `new` pour les builtins qui le requièrent, et l'interdit pour ceux qui ne le supportent pas.

```js
// ❌
const map = Map();
const sym = new Symbol('id');

// ✅
const map = new Map();
const sym = Symbol('id');
```
**Pourquoi :** Certains builtins (`Map`, `Set`, `WeakMap`) nécessitent `new`, d'autres (`Symbol`, `BigInt`) l'interdisent. Mélanger cause des `TypeError` au runtime.

---

### 24. `prefer-array-flat`
Préfère `Array#flat()` aux techniques legacy.

```js
// ❌
const flat = [].concat(...nested);
const flat2 = nested.reduce((a, b) => a.concat(b), []);

// ✅
const flat = nested.flat();
```
**Pourquoi :** `Array#flat()` est natif depuis ES2019. Les alternatives sont plus longues et moins lisibles.

---

### 25. `prefer-array-flat-map`
Préfère `.flatMap()` à `.map().flat()`.

```js
// ❌
const result = arr.map(x => getItems(x)).flat();

// ✅
const result = arr.flatMap(x => getItems(x));
```
**Pourquoi :** `.flatMap()` fait un seul passage au lieu de deux. Plus concis et plus performant.

---

### 26. `prefer-at`
Préfère `.at()` pour l'accès par index (surtout négatif).

```js
// ❌
const last = arr[arr.length - 1];
const char = str.charAt(0);

// ✅
const last = arr.at(-1);
const char = str.at(0);
```
**Pourquoi :** `.at(-1)` est plus lisible que `arr[arr.length - 1]` et fonctionne sur tous les indexables.

---

### 27. `prefer-includes`
Préfère `.includes()` à `.indexOf() !== -1`.

```js
// ❌
if (arr.indexOf(item) !== -1) { }

// ✅
if (arr.includes(item)) { }
```
**Pourquoi :** `.includes()` retourne un booléen — pas besoin de comparer à `-1`. Plus lisible et gère `NaN`.

---

### 28. `prefer-array-find`
Préfère `.find()` à `.filter()[0]`.

```js
// ❌
const first = items.filter(x => x.active)[0];

// ✅
const first = items.find(x => x.active);
```
**Pourquoi :** `.filter()[0]` parcourt tout le tableau pour ne garder que le premier élément. `.find()` s'arrête au premier match.

---

### 29. `prefer-array-some`
Préfère `.some()` à `.filter().length` ou `.find() !== undefined`.

```js
// ❌
if (items.filter(x => x.active).length > 0) { }

// ✅
if (items.some(x => x.active)) { }
```
**Pourquoi :** `.some()` court-circuite au premier match. `.filter().length` crée un tableau intermédiaire complet.

---

### 30. `prefer-array-index-of`
Préfère `.indexOf()` à `.findIndex()` pour les recherches de valeur simple.

```js
// ❌
const idx = arr.findIndex(x => x === 'foo');

// ✅
const idx = arr.indexOf('foo');
```
**Pourquoi :** `.indexOf()` est plus simple et exprime mieux l'intention de chercher une valeur exacte (pas un prédicat).

---

### 31. `prefer-object-from-entries`
Préfère `Object.fromEntries()` pour transformer une liste de paires clé/valeur en objet.

```js
// ❌
const obj = {};
pairs.forEach(([k, v]) => { obj[k] = v; });

// ✅
const obj = Object.fromEntries(pairs);
```
**Pourquoi :** `Object.fromEntries()` est déclaratif et immutable — pas de mutation d'un objet vide.

---

### 32. `prefer-spread`
Préfère le spread à `Array.from()`, `.concat()`, `.slice()`.

```js
// ❌
const copy = Array.from(set);
const merged = arr1.concat(arr2);
const clone = arr.slice();

// ✅
const copy = [...set];
const merged = [...arr1, ...arr2];
const clone = [...arr];
```
**Pourquoi :** Le spread est la syntaxe standard ES6 pour copier/fusionner. Plus lisible et cohérent.

---

### 33. `prefer-string-replace-all`
Préfère `String#replaceAll()` au regex global.

```js
// ❌
const result = str.replace(/foo/g, 'bar');

// ✅
const result = str.replaceAll('foo', 'bar');
```
**Pourquoi :** `.replaceAll()` est plus explicite et n'a pas besoin d'échapper les caractères regex.

---

### 34. `prefer-string-slice`
Préfère `.slice()` à `.substr()` et `.substring()`.

```js
// ❌
const part = str.substring(1, 3);
const end = str.substr(2);

// ✅
const part = str.slice(1, 3);
const end = str.slice(2);
```
**Pourquoi :** `.substr()` est déprécié, `.substring()` a un comportement surprenant avec les indices négatifs. `.slice()` est cohérent et supporte les indices négatifs.

---

### 35. `prefer-string-starts-ends-with`
Préfère `.startsWith()`/`.endsWith()` à un test regex.

```js
// ❌
if (/^foo/.test(str)) { }
if (/bar$/.test(str)) { }

// ✅
if (str.startsWith('foo')) { }
if (str.endsWith('bar')) { }
```
**Pourquoi :** Plus lisible, pas d'échappement regex, et exprime l'intention directement.

---

### 36. `prefer-string-trim-start-end`
Préfère `.trimStart()`/`.trimEnd()` à `.trimLeft()`/`.trimRight()`.

```js
// ❌
const s = str.trimLeft();

// ✅
const s = str.trimStart();
```
**Pourquoi :** `trimLeft`/`trimRight` sont des alias legacy. `trimStart`/`trimEnd` sont les noms standards (ES2019).

---

### 37. `prefer-date-now`
Préfère `Date.now()` pour obtenir le timestamp.

```js
// ❌
const ts = new Date().getTime();
const ts2 = +new Date();

// ✅
const ts = Date.now();
```
**Pourquoi :** `Date.now()` est plus explicite, ne crée pas d'objet `Date` inutile, et évite la coercion implicite.

---

### 38. `prefer-math-trunc`
Préfère `Math.trunc()` aux opérateurs bit-à-bit pour tronquer.

```js
// ❌
const int = value | 0;
const int2 = ~~value;

// ✅
const int = Math.trunc(value);
```
**Pourquoi :** Les opérateurs bit-à-bit convertissent en int32, tronquant silencieusement les grands nombres. `Math.trunc()` est explicite et sûr.

---

### 39. `prefer-modern-math-apis`
Préfère les APIs `Math` modernes aux patterns legacy.

```js
// ❌
const hyp = Math.sqrt(a * a + b * b);
const log2 = Math.log(x) / Math.LN2;

// ✅
const hyp = Math.hypot(a, b);
const log2 = Math.log2(x);
```
**Pourquoi :** Les APIs modernes sont plus précises (pas d'erreurs de float intermédiaires) et auto-documentées.

---

### 40. `prefer-number-properties`
Préfère les propriétés statiques de `Number` aux globales.

```js
// ❌
if (isNaN(x)) { }
const n = parseInt('42', 10);

// ✅
if (Number.isNaN(x)) { }
const n = Number.parseInt('42', 10);
```
**Pourquoi :** Les globales `isNaN`, `parseInt`, `parseFloat` sont legacy. `Number.isNaN` ne fait pas de coercion implicite (plus sûr).

---

### 41. `prefer-reflect-apply`
Préfère `Reflect.apply()` à `Function#apply()`.

```js
// ❌
Math.max.apply(null, args);

// ✅
Reflect.apply(Math.max, null, args);
```
**Pourquoi :** `Reflect.apply` ne peut pas être écrasé par un prototype modifié. Plus robuste dans un contexte multi-realm.

---

### 42. `prefer-prototype-methods`
Préfère emprunter les méthodes du prototype plutôt qu'à une instance.

```js
// ❌
const hasOwn = {}.hasOwnProperty.call(obj, 'key');

// ✅
const hasOwn = Object.prototype.hasOwnProperty.call(obj, 'key');
```
**Pourquoi :** Créer un objet juste pour emprunter une méthode est du gaspillage. Le prototype est la source canonique.

---

### 43. `prefer-structured-clone`
Préfère `structuredClone` pour le deep clone.

```js
// ❌
const copy = JSON.parse(JSON.stringify(obj));

// ✅
const copy = structuredClone(obj);
```
**Pourquoi :** `JSON.parse(JSON.stringify())` perd les `Date`, `RegExp`, `undefined`, `Map`, `Set`, et les références circulaires. `structuredClone` gère tout ça.

---

### 44. `prefer-global-this`
Préfère `globalThis` à `window`, `self`, `global`.

```js
// ❌
const value = window.innerWidth;
const g = typeof global !== 'undefined' ? global : self;

// ✅
const value = globalThis.innerWidth;
```
**Pourquoi :** `globalThis` est l'accès universel au global — fonctionne dans Node, navigateur, workers, Deno.

---

### 45. `prefer-native-coercion-functions`
Préfère les fonctions de coercion natives directement.

```js
// ❌
const nums = strings.map(s => Number(s));

// ✅
const nums = strings.map(Number);
```
**Pourquoi :** `Number`, `String`, `Boolean` sont déjà des fonctions de coercion — pas besoin de wrapper arrow.

---

### 46. `prefer-code-point`
Préfère `codePointAt()` à `charCodeAt()`.

```js
// ❌
const code = str.charCodeAt(0);

// ✅
const code = str.codePointAt(0);
```
**Pourquoi :** `charCodeAt` ne gère pas les caractères au-delà de U+FFFF (emojis, CJK rares). `codePointAt` gère tout Unicode.

---

### 47. `prefer-negative-index`
Préfère l'index négatif quand c'est possible.

```js
// ❌
const last = str.slice(str.length - 3);

// ✅
const last = str.slice(-3);
```
**Pourquoi :** L'index négatif est plus concis et évite la redondance de `str.length -`.

---

### 48. `prefer-top-level-await`
Préfère le top-level `await` dans les modules ESM.

```js
// ❌
async function main() {
  const data = await loadData();
  console.log(data);
}
main();

// ✅
const data = await loadData();
console.log(data);
```
**Pourquoi :** Le top-level `await` (ESM) est plus simple et évite le pattern IIFE async. Les consommateurs du module attendent automatiquement.

---

### 49. `prefer-set-has`
Préfère `Set#has()` à `Array#includes()` pour les lookups fréquents.

```js
// ❌
const allowed = ['admin', 'editor', 'viewer'];
if (allowed.includes(role)) { }

// ✅
const allowed = new Set(['admin', 'editor', 'viewer']);
if (allowed.has(role)) { }
```
**Pourquoi :** `Set#has()` est O(1) vs O(n) pour `Array#includes()`. Important pour les listes consultées en boucle.

---

### 50. `prefer-set-size`
Préfère `Set#size` à la conversion en array + `.length`.

```js
// ❌
const count = [...mySet].length;

// ✅
const count = mySet.size;
```
**Pourquoi :** Convertir un Set en array juste pour `.length` crée une allocation inutile. `.size` est O(1).

---

### 51. `prefer-bigint-literals`
Préfère `BigInt` littéral au constructeur.

```js
// ❌
const big = BigInt(9007199254740991);

// ✅
const big = 9007199254740991n;
```
**Pourquoi :** Le littéral `n` est plus concis. Et `BigInt(number)` peut perdre de la précision si le number dépasse `Number.MAX_SAFE_INTEGER`.

---

### 52. `no-typeof-undefined`
Interdit `typeof x === 'undefined'` (sauf pour les variables non déclarées).

```js
// ❌
if (typeof value === 'undefined') { }

// ✅
if (value === undefined) { }
```
**Pourquoi :** Depuis ES6, `undefined` ne peut plus être redéfini. La comparaison directe est plus simple et plus claire.

---

### 53. `no-useless-undefined`
Interdit les `undefined` explicites inutiles.

```js
// ❌
return undefined;
let x = undefined;
fn(undefined);

// ✅
return;
let x;
fn();
```
**Pourquoi :** `undefined` est la valeur par défaut de JS. L'écrire explicitement est redondant et ajoute du bruit.

---

### 54. `no-zero-fractions`
Interdit `1.0` — préférer `1`.

```js
// ❌
const x = 1.0;
const y = 1.00;

// ✅
const x = 1;
```
**Pourquoi :** JS n'a pas de type distinct pour les floats. `1.0` est identique à `1` — la fraction zéro est du bruit.

---

### 55. `number-literal-case`
Force la casse correcte pour les littéraux numériques.

```js
// ❌
const hex = 0XFF;
const exp = 1E3;

// ✅
const hex = 0xFF;
const exp = 1e3;
```
**Pourquoi :** Les conventions : préfixe en minuscule (`0x`, `0b`, `0o`), exposant en minuscule (`e`), hex digits en majuscule (`0xFF`).

---

### 56. `numeric-separators-style`
Force le style de séparateurs numériques.

```js
// ❌
const n = 1000000;
const hex = 0xFFFFFF;

// ✅
const n = 1_000_000;
const hex = 0xFF_FF_FF;
```
**Pourquoi :** Les séparateurs numériques (ES2021) rendent les grands nombres lisibles en groupant les digits.

---

### 57. `prefer-response-static-json`
Préfère `Response.json()` à `new Response(JSON.stringify())`.

```js
// ❌
return new Response(JSON.stringify(data), {
  headers: { 'Content-Type': 'application/json' },
});

// ✅
return Response.json(data);
```
**Pourquoi :** `Response.json()` est plus concis, gère le Content-Type automatiquement, et est supporté depuis Node 20.

---

### 58. `consistent-date-clone`
Préfère passer `Date` directement au constructeur pour cloner.

```js
// ❌
const clone = new Date(date.getTime());
const clone2 = new Date(date.valueOf());

// ✅
const clone = new Date(date);
```
**Pourquoi :** `new Date(date)` est le moyen le plus simple et le plus lisible de cloner une Date.

---

### 59. `no-await-expression-member`
Interdit l'accès membre sur une expression `await`.

```js
// ❌
const value = (await fetch(url)).json();

// ✅
const response = await fetch(url);
const value = response.json();
```
**Pourquoi :** Chaîner sur `await` rend la lecture difficile et masque l'étape intermédiaire. La variable nommée documente l'intention.

---

### 60. `no-await-in-promise-methods`
Interdit `await` dans les paramètres de `Promise.all()` et similaires.

```js
// ❌
const results = await Promise.all([
  await fetchA(),
  await fetchB(),
]);

// ✅
const results = await Promise.all([
  fetchA(),
  fetchB(),
]);
```
**Pourquoi :** `await` dans `Promise.all` sérialise les appels — on perd le parallélisme. Les promises doivent être passées non-résolues.

---

### 61. `no-single-promise-in-promise-methods`
Interdit un seul élément dans `Promise.all()` etc.

```js
// ❌
const [result] = await Promise.all([fetchData()]);

// ✅
const result = await fetchData();
```
**Pourquoi :** `Promise.all([x])` avec un seul élément est du bruit. Un simple `await` suffit.

---

### 62. `no-unnecessary-await`
Interdit `await` sur des non-promises.

```js
// ❌
const x = await 42;
const y = await 'hello';

// ✅
const x = 42;
const y = 'hello';
```
**Pourquoi :** `await` sur une valeur non-promise la wrappe dans un microtask inutile. Ajoute de la latence sans valeur.

---

### 63. `no-unnecessary-polyfills`
Interdit les polyfills pour des features natives.

```js
// ❌
import 'core-js/modules/es.array.flat';
// (Array.flat est natif depuis ES2019)

// ✅
// Rien — utiliser le natif
```
**Pourquoi :** Inclure un polyfill pour une feature déjà supportée par l'environnement cible augmente le bundle pour rien.

---

### 64. `prefer-import-meta-properties`
Préfère `import.meta.dirname`/`filename` aux techniques legacy.

```js
// ❌
import { fileURLToPath } from 'node:url';
const __filename = fileURLToPath(import.meta.url);

// ✅
const filename = import.meta.filename;
```
**Pourquoi :** `import.meta.dirname`/`filename` sont natifs depuis Node 21.2. Les alternatives sont des workarounds obsolètes.

---

### 65. `prefer-class-fields`
Préfère les champs de classe aux assignations dans le constructeur.

```js
// ❌
class Foo {
  constructor() {
    this.name = 'default';
    this.count = 0;
  }
}

// ✅
class Foo {
  name = 'default';
  count = 0;
}
```
**Pourquoi :** Les champs de classe (ES2022) sont plus déclaratifs et séparent les valeurs par défaut de la logique du constructeur.

---

## Préférences de style

### 66. `catch-error-name`
Force un nom spécifique pour le paramètre de `catch`.

```js
// ❌
try { } catch (e) { console.error(e); }

// ✅
try { } catch (error) { console.error(error); }
```
**Pourquoi :** `error` est plus descriptif que `e`, `err`, ou `ex`. La convention uniforme facilite la recherche dans le codebase.

---

### 67. `prevent-abbreviations`
Interdit les abréviations courantes.

```js
// ❌
function handleEvt(e) {
  const btn = e.target;
  const val = btn.value;
}

// ✅
function handleEvent(event) {
  const button = event.target;
  const value = button.value;
}
```
**Pourquoi :** Les abréviations ajoutent une charge cognitive — chaque lecteur doit deviner ce que `btn`, `evt`, `val` signifient. Les noms complets sont auto-documentés.

---

### 68. `filename-case`
Force un style de casse pour les noms de fichiers.

```
// ❌
MyComponent.js
my_component.js

// ✅ (si kebab-case configuré)
my-component.js
```
**Pourquoi :** Les conventions de nommage de fichiers inconsistantes créent de la confusion et des problèmes sur les systèmes de fichiers case-sensitive.

---

### 69. `empty-brace-spaces`
Interdit les espaces dans les accolades vides.

```js
// ❌
const obj = {  };
class Foo {  }

// ✅
const obj = {};
class Foo {}
```
**Pourquoi :** Les espaces dans des accolades vides sont du bruit visuel. Un bloc vide est plus lisible compact.

---

### 70. `escape-case`
Force la casse des séquences d'échappement.

```js
// ❌
const tab = '\T';  // invalide
const hex = '\xff';

// ✅
const hex = '\xFF';
```
**Pourquoi :** Convention : les hex dans les échappements utilisent des majuscules pour la lisibilité (`\xFF` vs `\xff`).

---

### 71. `no-hex-escape`
Préfère les échappements Unicode aux hex.

```js
// ❌
const str = '\x41';

// ✅
const str = '\u0041';
```
**Pourquoi :** Les échappements Unicode sont plus universels et cohérents avec `\u{}` pour les code points élevés.

---

### 72. `no-console-spaces`
Interdit les espaces de début/fin dans les arguments de `console.log`.

```js
// ❌
console.log(' hello ');
console.log('value: ', value);

// ✅
console.log('hello');
console.log('value:', value);
```
**Pourquoi :** `console.log` ajoute automatiquement des espaces entre ses arguments. Les espaces manuels sont des doubles.

---

### 73. `switch-case-braces`
Force des accolades cohérentes dans les `case`.

```js
// ❌
switch (x) {
  case 'a':
    const y = 1;
    break;
}

// ✅
switch (x) {
  case 'a': {
    const y = 1;
    break;
  }
}
```
**Pourquoi :** Sans accolades, les `const`/`let` dans un `case` fuitent dans les autres cases — source classique de bugs.

---

### 74. `switch-case-break-position`
Force la position du `break` dans les `case`.

```js
// ❌ (mixed)
switch (x) {
  case 'a':
    doA();
    break;
  case 'b': {
    doB();
  } break;
}
```
**Pourquoi :** La cohérence de position du `break`/`return` dans les `case` rend le flux de contrôle prévisible.

---

### 75. `template-indent`
Corrige l'indentation dans les template literals multi-lignes.

```js
// ❌
function foo() {
  const html = `
    <div>
      <p>hello</p>
    </div>
  `;
}

// ✅
function foo() {
  const html = `
<div>
  <p>hello</p>
</div>
  `;
}
```
**Pourquoi :** L'indentation du code source se retrouve dans la chaîne. Pour les heredocs (HTML, SQL), le contenu ne devrait pas hériter de l'indentation du code.

---

### 76. `no-unreadable-array-destructuring`
Interdit le destructuring de tableau illisible.

```js
// ❌
const [,, third,,,, seventh] = arr;

// ✅
const third = arr[2];
const seventh = arr[6];
```
**Pourquoi :** Compter les virgules est une source d'erreur. L'accès par index est plus explicite au-delà de 2 éléments ignorés.

---

### 77. `consistent-destructuring`
Préfère utiliser les variables destructurées plutôt que les propriétés.

```js
// ❌
const { name } = user;
console.log(user.age); // user.age devrait aussi être destructuré

// ✅
const { name, age } = user;
console.log(age);
```
**Pourquoi :** Si on destructure un objet, autant destructurer toutes les propriétés utilisées. Mélanger les accès est incohérent.

---

### 78. `consistent-assert`
Force un style d'assertion cohérent avec `node:assert`.

```js
// ❌
assert(x === 42);

// ✅
assert.strictEqual(x, 42);
```
**Pourquoi :** `assert(expr)` ne donne aucun détail sur l'échec. `assert.strictEqual` montre la valeur attendue vs reçue.

---

### 79. `consistent-empty-array-spread`
Préfère un style cohérent pour le spread de ternaire dans un tableau.

```js
// ❌
const arr = [
  ...condition ? ['a'] : [],
];

// ✅
const arr = [
  ...(condition ? ['a'] : []),
];
```
**Pourquoi :** La parenthèse explicite autour de la ternaire clarifie la portée du spread.

---

### 80. `consistent-existence-index-check`
Force un style cohérent pour les checks d'existence avec `indexOf`.

```js
// ❌
if (arr.indexOf(x) >= 0) { }

// ✅
if (arr.indexOf(x) !== -1) { }
```
**Pourquoi :** Cohérence — toujours comparer à `-1` plutôt que `>= 0`, `> -1`, ou `~ indexOf`. Plus clair et standard.

---

### 81. `consistent-template-literal-escape`
Force un style cohérent pour échapper `${` dans les template literals.

```js
// ❌ (inconsistent)
const a = `\${x}`;
const b = `$\{y}`;

// ✅
const a = `\${x}`;
const b = `\${y}`;
```
**Pourquoi :** Il y a deux façons d'échapper l'interpolation : `\${` et `$\{`. Choisir un style et s'y tenir.

---

### 82. `explicit-length-check`
Force la comparaison explicite de `.length`/`.size`.

```js
// ❌
if (arr.length) { }
if (!items.length) { }

// ✅
if (arr.length > 0) { }
if (items.length === 0) { }
```
**Pourquoi :** `.length` est un nombre, pas un booléen. La comparaison explicite documente l'intention : "le tableau n'est pas vide" vs "le tableau est falsy".

---

### 83. `no-keyword-prefix`
Interdit les identifiants commençant par `new` ou `class`.

```js
// ❌
const newUser = createUser();
const classNames = ['foo'];

// ✅
const createdUser = createUser();
const cssClasses = ['foo'];
```
**Pourquoi :** `newUser` ressemble à un appel de constructeur. `classNames` est ambiguë dans un contexte ES6 avec le mot-clé `class`.

---

### 84. `no-null`
Interdit le littéral `null`.

```js
// ❌
let user = null;
if (user === null) { }

// ✅
let user;
if (user === undefined) { }
```
**Pourquoi :** JS a deux "vides" : `null` et `undefined`. Utiliser les deux crée de l'ambiguïté. Standardiser sur `undefined` simplifie les comparaisons.

---

### 85. `prefer-string-raw`
Préfère `String.raw` pour éviter les doubles backslashes.

```js
// ❌
const regex = '\\d+\\.\\d+';

// ✅
const regex = String.raw`\d+\.\d+`;
```
**Pourquoi :** `String.raw` désactive l'interprétation des échappements — plus lisible que les doubles backslashes.

---

### 86. `text-encoding-identifier-case`
Force la casse des identifiants d'encodage texte.

```js
// ❌
const decoder = new TextDecoder('UTF-8');

// ✅
const decoder = new TextDecoder('utf-8');
```
**Pourquoi :** La spec WHATWG utilise le minuscule (`utf-8`). La cohérence évite les surprises dans les comparaisons.

---

### 87. `relative-url-style`
Force un style cohérent pour les URL relatives.

```js
// ❌
const url = new URL('./file.js', import.meta.url);

// ✅
const url = new URL('file.js', import.meta.url);
```
**Pourquoi :** `./file.js` et `file.js` sont équivalents avec une base URL. La forme courte est préférée par cohérence.

---

### 88. `no-magic-array-flat-depth`
Interdit un nombre magique comme profondeur de `.flat()`.

```js
// ❌
const flat = arr.flat(3);

// ✅
const NESTING_DEPTH = 3;
const flat = arr.flat(NESTING_DEPTH);
// ou
const flat = arr.flat(Infinity);
```
**Pourquoi :** Un nombre magique dans `.flat(n)` ne documente pas pourquoi cette profondeur. `Infinity` (aplatir tout) ou une constante nommée sont plus clairs.

---

### 89. `no-unnecessary-array-flat-depth`
Interdit `.flat(1)` — c'est le comportement par défaut.

```js
// ❌
const flat = arr.flat(1);

// ✅
const flat = arr.flat();
```
**Pourquoi :** `1` est la profondeur par défaut de `.flat()`. L'écrire explicitement est redondant.

---

### 90. `no-unnecessary-array-splice-count`
Interdit `.length`/`Infinity` comme argument de `.splice()`.

```js
// ❌
arr.splice(2, arr.length);

// ✅
arr.splice(2);
```
**Pourquoi :** Sans deuxième argument, `splice` supprime tout à partir de l'index. `.length` ou `Infinity` sont du bruit.

---

### 91. `no-unnecessary-slice-end`
Interdit `.length`/`Infinity` comme fin de `.slice()`.

```js
// ❌
const rest = arr.slice(2, arr.length);

// ✅
const rest = arr.slice(2);
```
**Pourquoi :** `.slice(n)` sans fin va jusqu'au bout. `.length` est redondant.

---

### 92. `no-useless-length-check`
Interdit les checks de `.length` inutiles avant `.some()`/`.every()`.

```js
// ❌
if (arr.length > 0 && arr.some(x => x > 0)) { }

// ✅
if (arr.some(x => x > 0)) { }
```
**Pourquoi :** `.some()` retourne `false` sur un tableau vide. Le check `.length > 0` est redondant.

---

### 93. `no-useless-spread`
Interdit les spread inutiles.

```js
// ❌
const arr = [...[1, 2, 3]];
const obj = { ...{ a: 1 } };

// ✅
const arr = [1, 2, 3];
const obj = { a: 1 };
```
**Pourquoi :** Spreader un littéral crée une copie identique sans valeur ajoutée. C'est du bruit syntaxique.

---

### 94. `no-useless-fallback-in-spread`
Interdit les fallbacks inutiles dans les spread.

```js
// ❌
const obj = { ...(foo || {}) };

// ✅
const obj = { ...foo };
```
**Pourquoi :** Spreader `undefined`/`null` dans un objet littéral est un no-op en JS. Le `|| {}` est inutile.

---

### 95. `no-useless-promise-resolve-reject`
Interdit `Promise.resolve()`/`reject()` dans les fonctions async.

```js
// ❌
async function foo() {
  return Promise.resolve(42);
}

// ✅
async function foo() {
  return 42;
}
```
**Pourquoi :** Une fonction `async` wrappe déjà la valeur de retour dans une Promise. Le double-wrap est inutile.

---

### 96. `no-useless-collection-argument`
Interdit les arguments inutiles dans les constructeurs de collections.

```js
// ❌
const set = new Set([]);
const map = new Map(undefined);

// ✅
const set = new Set();
const map = new Map();
```
**Pourquoi :** Passer `[]`, `undefined`, ou `null` aux constructeurs de Set/Map est le comportement par défaut.

---

### 97. `no-useless-error-capture-stack-trace`
Interdit `Error.captureStackTrace()` inutile.

```js
// ❌
class MyError extends Error {
  constructor(message) {
    super(message);
    Error.captureStackTrace(this, MyError); // déjà fait par super()
  }
}

// ✅
class MyError extends Error {
  constructor(message) {
    super(message);
  }
}
```
**Pourquoi :** Quand on hérite de `Error`, `super()` capture déjà la stack trace. L'appel explicite est redondant.

---

### 98. `no-useless-iterator-to-array`
Interdit `.toArray()` inutile sur les itérateurs.

```js
// ❌
for (const x of iter.toArray()) { }

// ✅
for (const x of iter) { }
```
**Pourquoi :** `for...of` accepte tout itérable. Convertir en array d'abord crée une allocation inutile.

---

## Strings

### 99. `prefer-node-protocol`
Préfère le préfixe `node:` pour les modules Node.js built-in.

```js
// ❌
import fs from 'fs';
import path from 'path';

// ✅
import fs from 'node:fs';
import path from 'node:path';
```
**Pourquoi :** Le préfixe `node:` distingue explicitement les builtins des packages npm. Évite les conflits de noms (ex: un npm `path`).

---

### 100. `prefer-module`
Préfère ESM à CommonJS.

```js
// ❌
const fs = require('fs');
module.exports = { foo };

// ✅
import fs from 'node:fs';
export { foo };
```
**Pourquoi :** ESM est le standard JS, supporte le tree-shaking, le top-level await, et l'analyse statique.

---

### 101. `import-style`
Force un style d'import spécifique par module.

```js
// ❌ (si configuré pour namespace)
import { join, resolve } from 'node:path';

// ✅
import path from 'node:path';
path.join(a, b);
```
**Pourquoi :** Certains modules (comme `path`) sont plus lisibles en namespace. La configuration permet de fixer un style par module.

---

### 102. `no-abusive-eslint-disable`
Interdit les `eslint-disable` sans spécifier de règle.

```js
// ❌
// eslint-disable-next-line
const x = eval('1 + 1');

// ✅
// eslint-disable-next-line no-eval
const x = eval('1 + 1');
```
**Pourquoi :** Désactiver toutes les règles d'un coup est dangereux — on peut masquer des bugs sans s'en rendre compte.

---

### 103. `no-anonymous-default-export`
Interdit les exports par défaut anonymes.

```js
// ❌
export default function() { }
export default class { }

// ✅
export default function myFunction() { }
```
**Pourquoi :** Les exports anonymes compliquent le debugging (pas de nom dans la stack trace) et le refactoring (pas d'identifiant à rechercher).

---

### 104. `no-named-default`
Interdit l'import nommé du default export, et vice versa.

```js
// ❌
import { default as foo } from './module';

// ✅
import foo from './module';
```
**Pourquoi :** `import { default as foo }` est redondant avec `import foo`. La forme courte est standard.

---

### 105. `prefer-export-from`
Préfère `export...from` pour les ré-exports.

```js
// ❌
import { foo } from './module';
export { foo };

// ✅
export { foo } from './module';
```
**Pourquoi :** `export...from` évite d'importer dans le scope local. Plus concis et l'intention de ré-export est explicite.

---

### 106. `require-module-attributes`
Force les attributs d'import pour les modules non-JS.

```js
// ❌
import data from './data.json';

// ✅
import data from './data.json' with { type: 'json' };
```
**Pourquoi :** Les attributs d'import spécifient le type MIME attendu, améliorant la sécurité et la compatibilité.

---

### 107. `require-module-specifiers`
Force les spécificateurs non-vides dans les imports/exports.

```js
// ❌
export {} from './module';
import {} from './module';

// ✅
export { foo } from './module';
import './module'; // side-effect import explicite
```
**Pourquoi :** Un import/export avec des spécificateurs vides est du bruit — utiliser un import side-effect si c'est l'intention.

---

## Arrays

### 108. `no-array-callback-reference`
Interdit le passage direct d'une référence de fonction comme callback d'itérateur.

```js
// ❌
const numbers = ['1', '2', '3'].map(Number.parseInt);

// ✅
const numbers = ['1', '2', '3'].map(x => Number.parseInt(x));
```
**Pourquoi :** `map` passe 3 arguments (value, index, array). `parseInt` utilise le 2e comme radix : `parseInt('3', 2)` → `NaN`.

---

### 109. `no-array-method-this-argument`
Interdit le paramètre `thisArg` des méthodes d'array.

```js
// ❌
arr.filter(function(x) { return this.check(x); }, context);

// ✅
arr.filter(x => context.check(x));
```
**Pourquoi :** L'argument `thisArg` est un vestige pré-arrow-function. Les arrow functions capturent `this` naturellement.

---

### 110. `no-array-reduce`
Interdit `Array#reduce()` et `Array#reduceRight()`.

```js
// ❌
const sum = arr.reduce((acc, x) => acc + x, 0);

// ✅
let sum = 0;
for (const x of arr) { sum += x; }
```
**Pourquoi :** `reduce` est souvent plus difficile à lire qu'une boucle explicite. L'accumulateur et la valeur initiale ajoutent de la charge cognitive. (Règle controversée — souvent désactivée pour les cas simples.)

---

### 111. `no-array-reverse`
Préfère `Array#toReversed()` à `Array#reverse()`.

```js
// ❌
const reversed = arr.reverse(); // mute arr !

// ✅
const reversed = arr.toReversed(); // arr inchangé
```
**Pourquoi :** `.reverse()` mute le tableau en place — source classique de bugs. `.toReversed()` (ES2023) retourne une copie.

---

### 112. `no-array-sort`
Préfère `Array#toSorted()` à `Array#sort()`.

```js
// ❌
const sorted = arr.sort((a, b) => a - b); // mute arr !

// ✅
const sorted = arr.toSorted((a, b) => a - b);
```
**Pourquoi :** `.sort()` mute le tableau en place. `.toSorted()` (ES2023) retourne une copie — immutabilité par défaut.

---

### 113. `prefer-single-call`
Combine plusieurs appels `.push()`, `.classList.add()`, etc. en un seul.

```js
// ❌
arr.push('a');
arr.push('b');
arr.push('c');

// ✅
arr.push('a', 'b', 'c');
```
**Pourquoi :** Un seul appel est plus performant (moins d'overhead de dispatch) et plus lisible.

---

### 114. `prefer-simple-condition-first`
Préfère la condition la plus simple en premier dans les expressions logiques.

```js
// ❌
if (complexFunction() && isEnabled) { }

// ✅
if (isEnabled && complexFunction()) { }
```
**Pourquoi :** Short-circuit : mettre la condition simple en premier évite le calcul coûteux si la simple est `false`.

---

### 115. `require-array-join-separator`
Force l'argument séparateur de `Array#join()`.

```js
// ❌
const str = arr.join();

// ✅
const str = arr.join(',');
```
**Pourquoi :** `join()` sans argument utilise `,` par défaut — peu de développeurs le savent. L'écrire explicitement évite les surprises.

---

### 116. `require-number-to-fixed-digits-argument`
Force l'argument de `Number#toFixed()`.

```js
// ❌
const str = num.toFixed();

// ✅
const str = num.toFixed(0);
```
**Pourquoi :** `toFixed()` sans argument utilise `0` par défaut. L'écrire explicitement rend l'intention claire (arrondi à l'entier).

---

## Erreurs

### 117. `error-message`
Force un message lors de la création d'erreurs.

```js
// ❌
throw new Error();
throw new TypeError();

// ✅
throw new Error('Connection failed: server unreachable');
throw new TypeError('Expected string, got number');
```
**Pourquoi :** Une erreur sans message rend le debugging pénible — pas de contexte dans la stack trace.

---

### 118. `throw-new-error`
Force `new` à la création d'erreurs.

```js
// ❌
throw Error('oops');

// ✅
throw new Error('oops');
```
**Pourquoi :** `Error()` et `new Error()` sont fonctionnellement identiques, mais `new` est la convention standard et rend l'intention d'instanciation explicite.

---

### 119. `custom-error-definition`
Force la bonne façon de sous-classer `Error`.

```js
// ❌
class MyError extends Error {
  constructor(message) {
    super(message);
    this.name = 'MyError';
  }
}

// ✅
class MyError extends Error {
  name = 'MyError';
}
```
**Pourquoi :** Assigner `this.name` dans le constructeur est le pattern legacy. Les champs de classe sont plus propres.

---

### 120. `prefer-type-error`
Force `TypeError` dans les conditions de type.

```js
// ❌
if (typeof x !== 'string') {
  throw new Error('Expected string');
}

// ✅
if (typeof x !== 'string') {
  throw new TypeError('Expected string');
}
```
**Pourquoi :** `TypeError` est le type d'erreur sémantiquement correct quand un argument est du mauvais type.

---

### 121. `prefer-optional-catch-binding`
Préfère omettre le paramètre `catch` quand il n'est pas utilisé.

```js
// ❌
try { } catch (error) {
  handleFailure();
}

// ✅
try { } catch {
  handleFailure();
}
```
**Pourquoi :** Si l'erreur n'est pas utilisée, la nommer est du bruit. Le catch sans binding (ES2019) est plus propre.

---

### 122. `no-process-exit`
Interdit `process.exit()`.

```js
// ❌
if (error) { process.exit(1); }

// ✅
if (error) { throw error; }
```
**Pourquoi :** `process.exit()` court-circuite le nettoyage (streams, connections, handlers `exit`). Lancer une erreur laisse le process se terminer proprement.

---

### 123. `no-instanceof-builtins`
Interdit `instanceof` avec les builtins.

```js
// ❌
if (err instanceof Error) { }
if (arr instanceof Array) { }

// ✅
if (err instanceof Error) { } // OK pour Error
if (Array.isArray(arr)) { }   // Array.isArray pour les arrays
```
**Pourquoi :** `instanceof` échoue entre realms (iframes, vm modules). Les méthodes statiques (`Array.isArray`) sont fiables cross-realm.

---

## DOM / Browser

### 124. `prefer-add-event-listener`
Préfère `.addEventListener()` aux propriétés `on*`.

```js
// ❌
element.onclick = handler;

// ✅
element.addEventListener('click', handler);
```
**Pourquoi :** Les propriétés `on*` n'acceptent qu'un seul handler — le second écrase le premier. `addEventListener` permet plusieurs listeners.

---

### 125. `no-invalid-remove-event-listener`
Interdit les expressions comme argument de `removeEventListener`.

```js
// ❌
el.removeEventListener('click', fn.bind(this));

// ✅
const handler = fn.bind(this);
el.addEventListener('click', handler);
el.removeEventListener('click', handler);
```
**Pourquoi :** `.bind()` crée une nouvelle référence à chaque appel. `removeEventListener` avec une référence différente est un no-op silencieux.

---

### 126. `prefer-dom-node-append`
Préfère `Node#append()` à `Node#appendChild()`.

```js
// ❌
parent.appendChild(child);

// ✅
parent.append(child);
```
**Pourquoi :** `append()` accepte plusieurs arguments et des strings. `appendChild()` n'accepte qu'un seul Node.

---

### 127. `prefer-dom-node-remove`
Préfère `childNode.remove()` à `parentNode.removeChild(childNode)`.

```js
// ❌
element.parentNode.removeChild(element);

// ✅
element.remove();
```
**Pourquoi :** `.remove()` est plus concis et n'a pas besoin de référencer le parent.

---

### 128. `prefer-dom-node-dataset`
Préfère `.dataset` à `getAttribute('data-*')`.

```js
// ❌
element.setAttribute('data-id', '42');
const id = element.getAttribute('data-id');

// ✅
element.dataset.id = '42';
const id = element.dataset.id;
```
**Pourquoi :** `.dataset` est l'API typée pour les attributs `data-*`. Plus concis et avec auto-conversion camelCase.

---

### 129. `prefer-dom-node-text-content`
Préfère `.textContent` à `.innerText`.

```js
// ❌
const text = element.innerText;

// ✅
const text = element.textContent;
```
**Pourquoi :** `.innerText` déclenche un reflow (lent) et ne retourne que le texte visible. `.textContent` est rapide et retourne tout le texte.

---

### 130. `prefer-modern-dom-apis`
Préfère les APIs DOM modernes.

```js
// ❌
parent.insertBefore(newNode, referenceNode);
parent.replaceChild(newNode, oldNode);

// ✅
referenceNode.before(newNode);
oldNode.replaceWith(newNode);
```
**Pourquoi :** Les APIs modernes (`.before()`, `.after()`, `.replaceWith()`) sont plus lisibles et n'ont pas besoin du parent.

---

### 131. `prefer-query-selector`
Préfère `.querySelector()` à `.getElementById()`.

```js
// ❌
const el = document.getElementById('main');
const items = document.getElementsByClassName('item');

// ✅
const el = document.querySelector('#main');
const items = document.querySelectorAll('.item');
```
**Pourquoi :** `querySelector`/`querySelectorAll` acceptent tout sélecteur CSS. API unifiée vs 4 méthodes différentes.

---

### 132. `prefer-keyboard-event-key`
Préfère `KeyboardEvent#key` à `keyCode`.

```js
// ❌
if (event.keyCode === 13) { }

// ✅
if (event.key === 'Enter') { }
```
**Pourquoi :** `keyCode` est déprécié et dépend du layout clavier. `.key` est une chaîne lisible et standard.

---

### 133. `prefer-classlist-toggle`
Préfère `classList.toggle()` aux if/else avec `add`/`remove`.

```js
// ❌
if (condition) {
  element.classList.add('active');
} else {
  element.classList.remove('active');
}

// ✅
element.classList.toggle('active', condition);
```
**Pourquoi :** `.toggle(name, force)` fait exactement ça en une ligne.

---

### 134. `prefer-blob-reading-methods`
Préfère les méthodes de `Blob` à `FileReader`.

```js
// ❌
const reader = new FileReader();
reader.readAsText(blob);
reader.onload = () => console.log(reader.result);

// ✅
const text = await blob.text();
```
**Pourquoi :** `Blob#text()` et `Blob#arrayBuffer()` retournent des Promises — plus propre que l'API callback de FileReader.

---

### 135. `prefer-event-target`
Préfère `EventTarget` à `EventEmitter`.

```js
// ❌
import { EventEmitter } from 'node:events';
class MyEmitter extends EventEmitter { }

// ✅
class MyEmitter extends EventTarget { }
```
**Pourquoi :** `EventTarget` est un standard web universel (browser + Node). `EventEmitter` est spécifique à Node.

---

### 136. `no-document-cookie`
Interdit l'accès direct à `document.cookie`.

```js
// ❌
document.cookie = 'name=value';
const cookies = document.cookie;

// ✅
// Utiliser une bibliothèque comme js-cookie ou CookieStore API
```
**Pourquoi :** `document.cookie` est une API archaïque à l'interface string. La CookieStore API ou une lib typée est plus sûre.

---

### 137. `no-invalid-fetch-options`
Interdit les options invalides dans `fetch()`.

```js
// ❌
fetch(url, { timeout: 5000 });  // "timeout" n'existe pas dans RequestInit
fetch(url, { body: 'data', method: 'GET' }); // GET ne peut pas avoir de body

// ✅
fetch(url, { signal: AbortSignal.timeout(5000) });
fetch(url, { body: 'data', method: 'POST' });
```
**Pourquoi :** `fetch` ignore silencieusement les options invalides. Capter ces erreurs au lint évite des bugs runtime subtils.

---

### 138. `require-post-message-target-origin`
Force `targetOrigin` dans `postMessage()`.

```js
// ❌
window.postMessage(data);

// ✅
window.postMessage(data, 'https://example.com');
```
**Pourquoi :** Sans `targetOrigin`, le message est envoyé à n'importe quelle origine — risque de fuite de données cross-origin.

---

## Regex

### 139. `better-regex`
Améliore les regex en les rendant plus courtes et plus sûres.

```js
// ❌
const re = /[0-9]/;
const re2 = /[a-zA-Z0-9_]/;

// ✅
const re = /\d/;
const re2 = /\w/;
```
**Pourquoi :** Les classes de caractères raccourcies (`\d`, `\w`, `\s`) sont standard et plus lisibles.

---

### 140. `prefer-regexp-test`
Préfère `RegExp#test()` à `String#match()` quand on ne veut que le booléen.

```js
// ❌
if (str.match(/pattern/)) { }

// ✅
if (/pattern/.test(str)) { }
```
**Pourquoi :** `.test()` retourne un booléen directement. `.match()` crée un objet de résultat inutile.

---

## Modules / Imports

### 141. `no-empty-file`
Interdit les fichiers vides.

```js
// ❌
// (fichier vide)

// ✅
export {};
// ou supprimer le fichier
```
**Pourquoi :** Un fichier vide est probablement un oubli. S'il est intentionnel, `export {}` le rend explicite.

---

### 142. `no-unused-properties`
Interdit les propriétés d'objet inutilisées (analyse statique limitée).

```js
// ❌
const config = {
  host: 'localhost',
  port: 3000,
  debug: true,  // jamais lu
};
console.log(config.host, config.port);
```
**Pourquoi :** Les propriétés inutilisées sont du code mort — elles ajoutent du bruit et de la confusion sur l'API réelle.

---

### 143. `string-content`
Force certains patterns dans le contenu des strings.

```js
// ❌ (si configuré pour remplacer '...' par '…')
const text = "It's great...";

// ✅
const text = "It's great…";
```
**Pourquoi :** Permet de forcer des conventions typographiques ou de remplacer des patterns dans les chaînes.

---

### 144. `expiring-todo-comments`
Force des conditions d'expiration sur les TODO comments.

```js
// ❌
// TODO: fix this later

// ✅
// TODO [2024-12-31]: fix this before year end
// TODO [>= node@20]: remove polyfill
```
**Pourquoi :** Les TODO sans deadline vivent éternellement. Une date d'expiration ou une condition technique force la résolution.

---

### 145. `no-array-reduce` (covered above as #110)

### 145. `prefer-default-parameters`
Préfère les paramètres par défaut aux réassignations.

```js
// ❌
function foo(x) {
  x = x || 'default';
}

// ✅
function foo(x = 'default') { }
```
**Pourquoi :** Les paramètres par défaut sont déclaratifs et gèrent correctement `undefined` (pas `0`, `""`, `false`).

---

### 146. `no-named-default` (covered above as #104)

---

## Règles restantes non encore couvertes

### `consistent-function-scoping` — déjà #1

### `no-array-reduce` — déjà #110

Pour être exhaustif, voici les dernières manquantes :

---

### `prefer-single-call` — déjà #113

### `no-immediate-mutation` — déjà #18

### `no-useless-error-capture-stack-trace` — déjà #97

### `no-useless-collection-argument` — déjà #96

### `no-useless-iterator-to-array` — déjà #98

---

## Résumé par difficulté d'implémentation

### Facile (TextCheck / pattern simple) — ~50 règles
`catch-error-name`, `empty-brace-spaces`, `error-message`, `escape-case`, `filename-case`, `no-abusive-eslint-disable`, `no-console-spaces`, `no-document-cookie`, `no-empty-file`, `no-hex-escape`, `no-new-buffer`, `no-null`, `no-process-exit`, `no-this-assignment`, `no-typeof-undefined`, `no-useless-undefined`, `no-zero-fractions`, `number-literal-case`, `numeric-separators-style`, `prefer-date-now`, `prefer-math-trunc`, `prefer-node-protocol`, `prefer-string-raw`, `prefer-string-replace-all`, `prefer-string-slice`, `prefer-string-starts-ends-with`, `prefer-string-trim-start-end`, `text-encoding-identifier-case`, `throw-new-error`, `require-array-join-separator`, `require-number-to-fixed-digits-argument`, `no-keyword-prefix`, `no-named-default`, `consistent-assert`, `consistent-template-literal-escape`, `switch-case-braces`, `prefer-regexp-test`, `prefer-optional-catch-binding`, `prefer-module`, `no-unreadable-iife`, `explicit-length-check`, `no-magic-array-flat-depth`, `no-unnecessary-array-flat-depth`, `no-unnecessary-array-splice-count`, `no-unnecessary-slice-end`

### Moyen (AstCheck / logique modérée) — ~60 règles
`consistent-destructuring`, `consistent-empty-array-spread`, `consistent-existence-index-check`, `consistent-function-scoping`, `no-array-callback-reference`, `no-array-for-each`, `no-array-method-this-argument`, `no-array-reduce`, `no-array-reverse`, `no-array-sort`, `no-await-expression-member`, `no-await-in-promise-methods`, `no-for-loop`, `no-lonely-if`, `no-negated-condition`, `no-negation-in-equality-check`, `no-nested-ternary`, `no-new-array`, `no-object-as-default-parameter`, `no-single-promise-in-promise-methods`, `no-static-only-class`, `no-thenable`, `no-unnecessary-await`, `no-unreadable-array-destructuring`, `no-useless-fallback-in-spread`, `no-useless-length-check`, `no-useless-promise-resolve-reject`, `no-useless-spread`, `no-useless-switch-case`, `new-for-builtins`, `prefer-add-event-listener`, `prefer-array-find`, `prefer-array-flat`, `prefer-array-flat-map`, `prefer-array-index-of`, `prefer-array-some`, `prefer-at`, `prefer-class-fields`, `prefer-classlist-toggle`, `prefer-code-point`, `prefer-default-parameters`, `prefer-dom-node-append`, `prefer-dom-node-dataset`, `prefer-dom-node-remove`, `prefer-dom-node-text-content`, `prefer-export-from`, `prefer-includes`, `prefer-logical-operator-over-ternary`, `prefer-math-min-max`, `prefer-modern-dom-apis`, `prefer-modern-math-apis`, `prefer-native-coercion-functions`, `prefer-negative-index`, `prefer-number-properties`, `prefer-object-from-entries`, `prefer-query-selector`, `prefer-set-has`, `prefer-set-size`, `prefer-spread`, `prefer-switch`, `prefer-ternary`, `prefer-type-error`

### Difficile (analyse complexe / inter-fichier) — ~36 règles
`better-regex`, `consistent-date-clone`, `custom-error-definition`, `expiring-todo-comments`, `import-style`, `isolated-functions`, `no-accessor-recursion`, `no-anonymous-default-export`, `no-immediate-mutation`, `no-instanceof-builtins`, `no-invalid-fetch-options`, `no-invalid-remove-event-listener`, `no-unnecessary-polyfills`, `no-unused-properties`, `prefer-bigint-literals`, `prefer-blob-reading-methods`, `prefer-event-target`, `prefer-global-this`, `prefer-import-meta-properties`, `prefer-keyboard-event-key`, `prefer-prototype-methods`, `prefer-reflect-apply`, `prefer-response-static-json`, `prefer-simple-condition-first`, `prefer-single-call`, `prefer-structured-clone`, `prefer-top-level-await`, `prevent-abbreviations`, `relative-url-style`, `require-module-attributes`, `require-module-specifiers`, `require-post-message-target-origin`, `string-content`, `switch-case-break-position`, `template-indent`, `no-useless-collection-argument`, `no-useless-error-capture-stack-trace`, `no-useless-iterator-to-array`
