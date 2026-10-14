# FFXIV Gils Toolkit

Application de bureau (macOS, Linux, Windows) pour gagner des gils à l'hôtel des
ventes de Final Fantasy XIV. Tout est en français, noms d'objets et de marchands
compris.

**Marché**

- **Scan** : objets moins chers sur un autre serveur ou centre de données que
  sur ton serveur principal, et que ton marché absorbera en quelques jours.
  Classés par gils par jour, pas par simple écart de prix.
- **Surveillance du marché** : objets surveillés en direct (WebSocket Universalis), avec
  notifications.
- **Liste de courses** : opportunités choisies dans un scan, regroupées par centre
  de données et serveur, avec budget, bénéfice attendu et cases « acheté » (le prix
  réellement payé est noté dans le journal).
- **Artisanat** : ce que tu sais fabriquer (niveaux importés du Lodestone) et qui
  rapporte. Chaque composant est acheté à l'HV, chez un marchand PNJ, récolté
  (option) ou fabriqué, selon le moins cher. Les meilleurs résultats sont
  recalculés sur les vraies annonces. Ingrédients vers la liste de courses et
  bouton « J'ai fabriqué » pour suivre la marge réelle à la vente.
- **Mes ventes** : toutes les annonces de tes servants, retrouvées en parcourant le
  marché de ton serveur (reconnues par leur nom), avec ton rang face à la
  concurrence, l'écart avec le moins cher, l'étiquette « Acheté » et ta marge pour
  ce qui vient de la liste de courses. Alerte quand une annonce sort de ton top X,
  bouton « Prix modifié » quand tu changes un prix en jeu. Les ventes sont
  détectées en quasi temps réel (WebSocket Universalis).
- **Journal des gains** : ventes détectées automatiquement (annonce disparue +
  vente identique dans l'historique Universalis), coût d'achat déduit dans
  l'ordre d'achat, bénéfice réel et marge sur 7 jours, 30 jours ou 1 an.
- **Recherche** et **Statistiques** du dernier scan (seul le dernier scan réussi est conservé).

**Dépenser**

- **Monnaies** : quoi acheter avec chaque monnaie (assignats, mémoquartz, sceaux,
  tribus alliées, JcJ, chasse…) pour le revendre au meilleur prix.
- **Revente PNJ** : objets vendus en gils par les marchands PNJ qui se revendent
  plus cher à l'hôtel des ventes.
- **Livraisons GC** : équipements à acheter pour les livrer contre des sceaux,
  comparés à la valeur réelle d'un sceau sur ton serveur.
- **Servants** : missions ciblées les plus rentables selon le niveau et le niveau
  d'objet / la perception de ton servant.
- **Désynthèse** : objets dont un résultat possible vaut plus que leur prix
  (meilleur cas : les taux ne sont pas publics).

## Le calcul du scan en 90 secondes

Tu achètes sur un serveur visité et revends toujours sur ton serveur principal.
Un écart de prix ne vaut rien si personne n'achète chez toi : tout est exprimé
en **unités que ton marché absorbe**.

1. **Taxes.** L'acheteur paie 5 % en plus du prix affiché ; le vendeur paie 0 à
   5 % selon la cité de son servant. Une unité coûte donc `annonce × 1,05` et
   rapporte `vente × (1 − taxe vendeur)`.
2. **Demande locale.** Sur les 7 derniers jours de ventes chez toi : unités
   vendues par jour (somme des quantités, pas des ventes) et prix médian pondéré
   par quantité, hors valeurs aberrantes. Il faut passer sous l'annonce crédible
   la moins chère (les annonces « troll » sous la moitié de la médiane sont
   ignorées) : prix de revente attendu `min(médiane, plancher − 1)`.
3. **Quoi acheter.** Les annonces s'achètent en entier. On prend les moins
   chères tant qu'elles restent sous le seuil de rentabilité et que le total ne
   dépasse pas ce que ton marché vend sur l'horizon de vente
   (`unités/jour × 3 jours` par défaut).
4. **Classement.** `bénéfice par trajet = unités × vente nette − coût`, et
   `gils par jour = bénéfice unitaire × min(unités/jour, unités ÷ horizon)`. Un
   badge de confiance résume la vitesse de vente et la concurrence.
5. **Trajets.** Un trajet regroupe tout ce qui vaut la peine sur un serveur ; le
   bénéfice horaire remplit les 140 places d'inventaire, les plus rentables
   d'abord.

Une unité HQ peut aussi répondre à une demande NQ ; l'app choisit ce qui paie le
mieux. Le scan préfiltre d'abord ~17 000 objets via l'API agrégée d'Universalis,
puis analyse en détail les 200 meilleurs. Tous les seuils sont dans
`src-tauri/src/profit.rs`.

## Lancer et compiler

Prérequis : [Rust](https://rustup.rs) (stable), Node.js 20+, pnpm et les
[dépendances système de Tauri](https://tauri.app/start/prerequisites/).

```sh
pnpm install
pnpm tauri dev      # lancement avec rechargement à chaud
pnpm tauri build    # paquet pour la plateforme courante
```

Vérifications :

```sh
pnpm typecheck && pnpm lint && pnpm format:check
pnpm rs:lint && pnpm rs:test
# de bout en bout contre les API réelles (réseau, ~2 min)
cargo test --manifest-path src-tauri/Cargo.toml live_ -- --ignored --nocapture
# outils « Dépenser » : jeu de données réel + top d'une monnaie sur un serveur
cargo run --manifest-path src-tauri/Cargo.toml -p spender-core --example live -- Moogle 41784
```

Organisation du code :

- `src-tauri/src` : scan, suivi, statistiques (SQLite) et commandes des outils
  « Dépenser » (`spender.rs`) ;
- `src-tauri/core` : crate `spender-core`, indépendante de Tauri, logique des
  outils « Dépenser » ;
- `src/` : interface React, outils « Dépenser » dans `src/spender`.

Les données (base SQLite, données du jeu) sont dans le dossier de données de
l'application (`~/Library/Application Support/app.ffxivtrader` sur macOS) ; les
journaux (7 jours) dans le dossier de logs du système. Paramètres → À propos →
« Ouvrir le dossier des journaux ».

## Versions

Pousser un tag de version compile toutes les cibles et crée une release GitHub
**brouillon** avec les installeurs (un `.dmg` universel Apple Silicon + Intel,
`.deb`, `.AppImage`, `.msi`) :

```sh
# mettre à jour "version" dans src-tauri/tauri.conf.json et package.json avant
git tag v0.3.0 && git push origin v0.3.0
```

La CI (`.github/workflows/ci.yml`) lance typecheck, lint, format, rustfmt,
clippy et les tests Rust à chaque push et pull request.

### Ouvrir un build non signé

- **macOS** : après l'avoir copiée dans Applications,
  `xattr -cr "/Applications/FFXIV Gils Toolkit.app"`, ou clic droit → **Ouvrir**
  → **Ouvrir** une fois.
- **Windows** : sur « Windows a protégé votre ordinateur », **Informations
  complémentaires** → **Exécuter quand même**.

### Activer la signature

Le workflow de release signe automatiquement si ces secrets existent dans le
dépôt :

- macOS : `APPLE_CERTIFICATE` (.p12 en base64), `APPLE_CERTIFICATE_PASSWORD`,
  `APPLE_SIGNING_IDENTITY`, et pour la notarisation `APPLE_ID`, `APPLE_PASSWORD`
  (mot de passe d'application), `APPLE_TEAM_ID`.
- Windows : `WINDOWS_CERTIFICATE` (.pfx en base64), `WINDOWS_CERTIFICATE_PASSWORD`.

## Données hors ligne

Si XIVAPI est injoignable, les noms, icônes et tailles de pile viennent d'un
extrait embarqué de la feuille des objets (`src-tauri/data/items-fallback.tsv.gz`,
issu de [ffxiv-datamining](https://github.com/xivapi/ffxiv-datamining)). À
régénérer après un patch avec `python3 scripts/gen_item_fallback.py`.

## Sources

- Prix : [universalis.app](https://universalis.app).
- Données du jeu : [XIVAPI](https://v2.xivapi.com) et
  [ffxiv-datamining](https://github.com/xivapi/ffxiv-datamining).
- Résultats de désynthèse : données communautaires de
  [Teamcraft](https://github.com/ffxiv-teamcraft/ffxiv-teamcraft).
- L'application ne fait que lire ces données ; elle n'envoie rien.

Code sous licence [MIT](LICENSE).

FFXIV Gils Toolkit est un outil de fan, ni affilié ni approuvé par Square Enix.
FINAL FANTASY XIV © SQUARE ENIX CO., LTD.
