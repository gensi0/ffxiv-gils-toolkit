# FFXIV Gils Toolkit

Application web (installable sur ordinateur et téléphone) pour gagner des gils à
l'hôtel des ventes de Final Fantasy XIV. Tout est en français, noms d'objets et de
marchands compris. Un serveur (une image Docker) fait le travail : scans, suivi
des ventes 24 h/24, notifications. Chaque joueur a son compte, et retrouve ses
données sur tous ses appareils.

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

## Héberger le serveur

L'image `ghcr.io/gensi0/ffxiv-gils-toolkit` contient le serveur et le client web.
Exemple avec Traefik : [`deploy/docker-compose.yml`](deploy/docker-compose.yml).

```sh
# INVITE_CODE : code à donner aux joueurs pour créer leur compte
docker compose up -d
docker compose pull && docker compose up -d   # mise à jour
```

Variables : `INVITE_CODE` (sans lui, inscriptions fermées), `DATA_DIR` (`/data`),
`PORT` (8080), `COOKIE_SECURE` (`true` ; `false` seulement pour tester en HTTP),
`RUST_LOG`.

Les données sont dans le volume `/data` : `accounts.db` (comptes, sessions,
abonnements push, clé VAPID), `users/<id>.db` (données de chaque compte, même
schéma que l'app de bureau), `shared.db` (caches Universalis et XIVAPI communs),
`icons/` et les données du jeu. Copie cohérente à chaud :
`docker compose exec gils-toolkit ffxiv-gils-server --backup /data/backup`.

Le serveur fait tourner en continu, pour chaque compte, le suivi de Mes ventes
(relevé toutes les 3 min, relevé complet du serveur toutes les 30 min, WebSocket
Universalis) et les scans programmés. Les requêtes Universalis de tous les
comptes partagent le même cache et la même limite de débit.

## Installer l'app

Ouvre l'adresse du serveur, crée ton compte avec le code d'invitation, puis :

- **Android (Chrome)** : menu ⋮ → **Ajouter à l'écran d'accueil** / **Installer
  l'application** ;
- **Ordinateur (Chrome, Edge)** : icône d'installation dans la barre d'adresse ;
- **iPhone (Safari)** : Partager → **Sur l'écran d'accueil**.

Puis Paramètres → Compte → Notifications sur cet appareil → **Activer** (à faire
sur chaque appareil). Sur téléphone, l'app s'ouvre en **mode compagnon** :
Accueil, Mes ventes, Liste de courses, Journal et Réglages.

## Développer

Prérequis : [Rust](https://rustup.rs) (stable), Node.js 22+, pnpm.

```sh
pnpm install
# serveur en local (HTTP, données dans ./data-dev)
DATA_DIR=./data-dev WEB_DIR=./dist INVITE_CODE=dev COOKIE_SECURE=false \
  cargo run --manifest-path server/Cargo.toml
pnpm dev            # client sur :1420, API relayée vers :8080 (GILS_API)
```

Vérifications :

```sh
pnpm typecheck && pnpm lint && pnpm format:check
cargo fmt --manifest-path server/Cargo.toml --check
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path server/Cargo.toml
```

Organisation du code :

- `server/` : serveur axum. `app.rs` (contexte par compte), `auth.rs` (comptes,
  sessions), `push.rs` (Web Push), `rpc.rs` (`POST /api/cmd/<commande>`, mêmes
  commandes que sous Tauri), événements par WebSocket sur `/api/events` ; le reste
  est la logique métier (scan, ventes, artisanat…) ;
- `src-tauri/core` : crate `spender-core`, logique des outils « Dépenser » et de
  l'artisanat, partagée ;
- `src/` : interface React. `src/lib/backend.ts` choisit le transport : HTTP +
  WebSocket dans le navigateur, commandes Tauri dans l'app de bureau ;
- `src-tauri/` : application de bureau et Android (Tauri), gardée pour un futur
  client lourd. Plus publiée à chaque version : workflow manuel « Clients
  lourds » (`.github/workflows/desktop.yml`).

## Versions

Mettre à jour la version dans `package.json` et `server/Cargo.toml`, puis :

```sh
git tag vX.Y.Z && git push origin vX.Y.Z
```

Le workflow Release construit l'image Docker, la publie sur GHCR (`X.Y.Z` et
`latest`) et crée une release GitHub brouillon. La CI lance typecheck, lint,
format, rustfmt, clippy, les tests Rust, et construit puis teste l'image Docker à
chaque pull request.

## Données hors ligne

Si XIVAPI est injoignable, les noms, icônes et tailles de pile viennent d'un
extrait embarqué de la feuille des objets (`server/data/items-fallback.tsv.gz`,
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
