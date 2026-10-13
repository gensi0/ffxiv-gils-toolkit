//! Construction du jeu de données : quels objets revendables s'échangent
//! contre quelle monnaie, chez quels marchands, à quel prix en monnaie.
//!
//! Sources (XIVAPI, en français) :
//! - `SpecialShop` : les échanges « monnaie → objet » (assignats, mémoquartz,
//!   tribus alliées, chasse, JcJ…) ;
//! - `GCScripShopItem` : les échanges contre des sceaux de grande compagnie ;
//! - `ENpcBase` et les feuilles de liaison (`TopicSelect`, `PreHandler`,
//!   `InclusionShop*`, `CustomTalk`, `GCShop`) : quels marchands ouvrent quelles
//!   boutiques. Une boutique sans marchand n'est plus accessible en jeu : elle
//!   est écartée.
//!
//! Seuls les échanges payés avec **une seule** monnaie, pour un objet
//! **revendable à l'hôtel des ventes** (liste `marketable` d'Universalis),
//! sont conservés.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http::Http;
use crate::universalis::Universalis;
use crate::xivapi::{icon_path, FieldsExt, Row, XivApi};
use crate::Result;

/// À incrémenter quand le format change : un fichier plus ancien est reconstruit.
pub const DATASET_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Assignats,
    Memoquartz,
    GrandeCompagnie,
    TribusAlliees,
    Jcj,
    Chasse,
    Autres,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Currency {
    pub item_id: u32,
    pub name: String,
    pub icon: Option<String>,
    pub category: Category,
    /// Nombre d'objets revendables (distincts) obtenables avec cette monnaie.
    pub item_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unlock {
    /// Raison lisible (« Quête : … », « Rang de réputation requis »…),
    /// absente quand le jeu ne la donne pas clairement.
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub currency: u32,
    pub item: u32,
    /// Quantité reçue par échange.
    pub quantity: u32,
    /// Coût en monnaie par échange.
    pub cost: u32,
    pub hq: bool,
    pub shops: Vec<String>,
    pub vendors: Vec<String>,
    pub unlock: Option<Unlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemMeta {
    pub name: String,
    pub icon: Option<String>,
}

/// Objet vendu en gils par un marchand PNJ et revendable à l'hôtel des ventes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GilOffer {
    pub item: u32,
    /// Prix d'achat au PNJ.
    pub price: u32,
    pub vendors: Vec<String>,
    pub unlock: Option<Unlock>,
}

/// Équipement accepté en livraison experte de grande compagnie.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Delivery {
    pub item: u32,
    pub item_level: u32,
    /// Sceaux reçus pour une livraison.
    pub seals: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VentureClass {
    Combat,
    Mineur,
    Botaniste,
    Pecheur,
}

/// Mission de servant ciblée (1 h, 1 jeton de mission).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Venture {
    pub item: u32,
    pub class: VentureClass,
    pub retainer_level: u32,
    /// Niveau d'objet moyen (combat) ou récolte (récolteurs) minimum.
    pub required_stat: u32,
    /// Seuils (niveau d'objet ou perception) des paliers 2 à 5.
    pub thresholds: Vec<u32>,
    /// Quantité rapportée à chaque palier (1 à 5).
    pub quantities: Vec<u32>,
}

/// Résultats possibles de la désynthèse d'un objet (sans taux connus).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Desynth {
    pub item: u32,
    pub results: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dataset {
    pub version: u32,
    pub built_at_ms: i64,
    pub currencies: Vec<Currency>,
    pub offers: Vec<Offer>,
    pub gil_offers: Vec<GilOffer>,
    pub deliveries: Vec<Delivery>,
    pub ventures: Vec<Venture>,
    pub desynths: Vec<Desynth>,
    pub items: HashMap<u32, ItemMeta>,
}

// ---------------------------------------------------------------------------
// Monnaies prises en charge

const SCRIPS: &[u32] = &[33913, 33914, 41784, 41785];
const PVP: &[u32] = &[36656];
const HUNT: &[u32] = &[26533, 27, 10307];
const OTHER: &[u32] = &[26807];
/// Sceaux de grande compagnie : Maelstrom (1), Deux Vipères (2), Immortels (3).
const GC_SEALS: [(u32, u32); 3] = [(1, 20), (2, 21), (3, 22)];

/// Dans les boutiques à assignats (`UseCurrencyType` 16), le coût n'est pas un
/// identifiant d'objet mais un index : 2/4 = assignats mauves artisan/récolteur,
/// 6/7 = orange. L'index 1 désigne le mémoquartz poétique (cf. `TomestonesItem`).
const SCRIP_INDEX: &[(u32, u32)] = &[(2, 33913), (4, 33914), (6, 41784), (7, 41785)];

// ---------------------------------------------------------------------------
// Données brutes et résolution (logique pure, testée sans réseau)

#[derive(Debug, Default)]
pub struct RawShopItem {
    pub items: Vec<u32>,
    pub counts: Vec<u32>,
    pub hq: Vec<bool>,
    pub cost_items: Vec<u32>,
    pub cost_counts: Vec<u32>,
    pub quest: u32,
    pub achievement: u32,
}

#[derive(Debug, Default)]
pub struct RawShop {
    pub id: u32,
    pub name: String,
    pub use_currency_type: u32,
    pub quest: u32,
    pub items: Vec<RawShopItem>,
}

/// Tout ce qu'il faut pour relier marchands et boutiques.
#[derive(Debug, Default)]
pub struct Links {
    /// Marchand → valeurs `ENpcData` non nulles.
    pub npc_data: HashMap<u32, Vec<u32>>,
    pub topic_select: HashMap<u32, Vec<u32>>,
    pub pre_handler: HashMap<u32, u32>,
    pub inclusion_shop: HashMap<u32, Vec<u32>>,
    pub inclusion_category: HashMap<u32, u32>,
    pub inclusion_series: HashMap<u32, Vec<u32>>,
    pub custom_talk: HashMap<u32, Vec<u32>>,
    /// Boutique de sceaux → grande compagnie.
    pub gc_shop: HashMap<u32, u32>,
}

/// Échange avant nommage (identifiants seulement).
#[derive(Debug, Clone, PartialEq)]
pub struct RawOffer {
    pub currency: u32,
    pub item: u32,
    pub quantity: u32,
    pub cost: u32,
    pub hq: bool,
    pub shop_name: String,
    pub vendors: BTreeSet<u32>,
    pub quest: u32,
    pub achievement: u32,
    /// Grade de grande compagnie requis (> 1), avec la compagnie.
    pub gc_rank: Option<(u32, u32)>,
}

pub fn cost_item(use_currency_type: u32, raw: u32, tomestones: &HashMap<u32, u32>) -> u32 {
    match use_currency_type {
        2 | 4 => tomestones.get(&raw).copied().unwrap_or(raw),
        16 => SCRIP_INDEX
            .iter()
            .find(|(i, _)| *i == raw)
            .map(|(_, item)| *item)
            .or_else(|| (raw == 1).then(|| tomestones.get(&1).copied()).flatten())
            .unwrap_or(raw),
        _ => raw,
    }
}

impl Links {
    /// Boutiques (`SpecialShop`) atteintes depuis une valeur `ENpcData`.
    fn special_shops(&self, value: u32, shops: &HashSet<u32>, depth: u8, out: &mut Vec<u32>) {
        if value == 0 || depth > 4 {
            return;
        }
        if shops.contains(&value) {
            out.push(value);
        } else if let Some(targets) = self.topic_select.get(&value) {
            for t in targets {
                self.special_shops(*t, shops, depth + 1, out);
            }
        } else if let Some(t) = self.pre_handler.get(&value) {
            self.special_shops(*t, shops, depth + 1, out);
        } else if let Some(categories) = self.inclusion_shop.get(&value) {
            for c in categories {
                let series = self.inclusion_category.get(c).copied().unwrap_or(0);
                for s in self.inclusion_series.get(&series).into_iter().flatten() {
                    self.special_shops(*s, shops, depth + 1, out);
                }
            }
        } else if let Some(args) = self.custom_talk.get(&value) {
            for a in args {
                self.special_shops(*a, shops, depth + 1, out);
            }
        }
    }

    /// Boutique → marchands qui l'ouvrent.
    pub fn shop_vendors(&self, shops: &HashSet<u32>) -> HashMap<u32, BTreeSet<u32>> {
        let mut out: HashMap<u32, BTreeSet<u32>> = HashMap::new();
        let mut found = Vec::new();
        for (npc, values) in &self.npc_data {
            for v in values {
                found.clear();
                self.special_shops(*v, shops, 0, &mut found);
                for s in &found {
                    out.entry(*s).or_default().insert(*npc);
                }
            }
        }
        out
    }

    /// Grande compagnie → marchands de sceaux (intendants).
    pub fn gc_vendors(&self) -> HashMap<u32, BTreeSet<u32>> {
        let mut out: HashMap<u32, BTreeSet<u32>> = HashMap::new();
        for (npc, values) in &self.npc_data {
            for v in values {
                if let Some(gc) = self.gc_shop.get(v) {
                    out.entry(*gc).or_default().insert(*npc);
                }
            }
        }
        out
    }
}

/// Échanges `SpecialShop` payés avec une seule monnaie suivie, pour un objet
/// revendable, dans une boutique ouverte par au moins un marchand.
pub fn special_shop_offers(
    shops: &[RawShop],
    links: &Links,
    tomestones: &HashMap<u32, u32>,
    currencies: &HashSet<u32>,
    marketable: &HashSet<u32>,
) -> Vec<RawOffer> {
    let ids: HashSet<u32> = shops.iter().map(|s| s.id).collect();
    let vendors = links.shop_vendors(&ids);
    let mut out = Vec::new();
    for shop in shops {
        let Some(shop_vendors) = vendors.get(&shop.id) else {
            continue;
        };
        for it in &shop.items {
            let item = it.items.first().copied().unwrap_or(0);
            // Les lots de plusieurs objets différents ne se comparent pas à un prix unitaire.
            if item == 0 || it.items.iter().skip(1).any(|&i| i != 0) || !marketable.contains(&item)
            {
                continue;
            }
            let costs: Vec<(u32, u32)> = it
                .cost_items
                .iter()
                .zip(&it.cost_counts)
                .filter(|(&c, &n)| c != 0 && n != 0)
                .map(|(&c, &n)| (cost_item(shop.use_currency_type, c, tomestones), n))
                .collect();
            let [(currency, cost)] = costs[..] else {
                continue;
            };
            if !currencies.contains(&currency) {
                continue;
            }
            out.push(RawOffer {
                currency,
                item,
                quantity: it.counts.first().copied().unwrap_or(1).max(1),
                cost,
                hq: it.hq.first().copied().unwrap_or(false),
                shop_name: shop.name.clone(),
                vendors: shop_vendors.clone(),
                quest: if it.quest != 0 { it.quest } else { shop.quest },
                achievement: it.achievement,
                gc_rank: None,
            });
        }
    }
    out
}

/// Fusionne les échanges identiques vendus par plusieurs boutiques. Un objet
/// accessible sans condition quelque part est considéré comme débloqué.
pub fn merge(raw: Vec<RawOffer>) -> Vec<RawOffer> {
    let mut by_key: HashMap<(u32, u32, u32, u32, bool), RawOffer> = HashMap::new();
    let mut shops: HashMap<(u32, u32, u32, u32, bool), BTreeSet<String>> = HashMap::new();
    for o in raw {
        let key = (o.currency, o.item, o.quantity, o.cost, o.hq);
        shops.entry(key).or_default().insert(o.shop_name.clone());
        match by_key.get_mut(&key) {
            None => {
                by_key.insert(key, o);
            }
            Some(existing) => {
                existing.vendors.extend(o.vendors);
                let o_free = o.quest == 0 && o.achievement == 0 && o.gc_rank.is_none();
                if o_free {
                    existing.quest = 0;
                    existing.achievement = 0;
                    existing.gc_rank = None;
                }
            }
        }
    }
    let mut out: Vec<RawOffer> = by_key
        .into_iter()
        .map(|(key, mut o)| {
            o.shop_name = shops[&key].iter().cloned().collect::<Vec<_>>().join(" · ");
            o
        })
        .collect();
    out.sort_by_key(|o| (o.currency, o.item, o.cost));
    out
}

/// Retire l'icône de quête (caractère d'usage privé) et met une majuscule.
pub fn clean_name(s: &str) -> String {
    let s: String = s
        .chars()
        .filter(|c| !('\u{E000}'..='\u{F8FF}').contains(c))
        .collect();
    let s = s.trim();
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Construction depuis le réseau

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub step: String,
    pub done: usize,
    pub total: usize,
}

const STEPS: usize = 13;

/// Résultats de désynthèse connus de la communauté (Teamcraft) :
/// `{ résultat: [objets sources] }`. Le jeu ne les contient pas.
const DESYNTH_URL: &str = "https://raw.githubusercontent.com/ffxiv-teamcraft/ffxiv-teamcraft/staging/libs/data/src/lib/json/desynth.json";

/// Inverse `{ résultat: [sources] }` en désynthèses par objet source.
pub fn invert_desynth(raw: &HashMap<String, Vec<u32>>, marketable: &HashSet<u32>) -> Vec<Desynth> {
    let mut by_source: HashMap<u32, BTreeSet<u32>> = HashMap::new();
    for (result, sources) in raw {
        let Ok(result) = result.parse::<u32>() else {
            continue;
        };
        for &src in sources {
            if marketable.contains(&src) {
                by_source.entry(src).or_default().insert(result);
            }
        }
    }
    let mut out: Vec<Desynth> = by_source
        .into_iter()
        .map(|(item, r)| Desynth {
            item,
            results: r.into_iter().collect(),
        })
        .collect();
    out.sort_by_key(|d| d.item);
    out
}

fn parse_shop(row: &Row) -> RawShop {
    let f = &row.fields;
    let items = f
        .get("Item")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|it| RawShopItem {
                    items: it.u32s_of("Item@as(raw)"),
                    counts: it.u32s_of("ReceiveCount"),
                    hq: it
                        .get("ReceiveHq")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().map(|b| b.as_bool().unwrap_or(false)).collect())
                        .unwrap_or_default(),
                    cost_items: it.u32s_of("ItemCost@as(raw)"),
                    cost_counts: it.u32s_of("CurrencyCost"),
                    quest: it.u32_of("Quest@as(raw)"),
                    achievement: it.u32_of("AchievementUnlock@as(raw)"),
                })
                .collect()
        })
        .unwrap_or_default();
    RawShop {
        id: row.row_id,
        name: clean_name(&f.str_of("Name")),
        use_currency_type: f.u32_of("UseCurrencyType"),
        quest: f.u32_of("Quest@as(raw)"),
        items,
    }
}

fn nonzero(v: Vec<u32>) -> Vec<u32> {
    v.into_iter().filter(|&x| x != 0).collect()
}

/// Construit le jeu de données complet (~30 s, ~120 requêtes).
pub async fn build(http: &Http, progress: &(dyn Fn(Progress) + Sync)) -> Result<Dataset> {
    let api = XivApi::new(http.clone());
    let universalis = Universalis::new(http.clone());
    let mut step = 0;
    let mut report = |label: &str| {
        step += 1;
        progress(Progress {
            step: label.to_string(),
            done: step,
            total: STEPS,
        });
    };

    report("Liste des objets revendables");
    let marketable: HashSet<u32> = universalis.marketable().await?.into_iter().collect();

    report("Boutiques d'échange");
    let shops: Vec<RawShop> = api
        .sheet(
            "SpecialShop",
            "Name,UseCurrencyType,Quest@as(raw),Item[].Item@as(raw),Item[].ReceiveCount,\
             Item[].ReceiveHq,Item[].ItemCost@as(raw),Item[].CurrencyCost,Item[].Quest@as(raw),\
             Item[].AchievementUnlock@as(raw)",
        )
        .await?
        .iter()
        .map(parse_shop)
        .collect();

    report("Monnaies");
    let tomestones: HashMap<u32, u32> = api
        .sheet("TomestonesItem", "Item@as(raw),Tomestones@as(raw)")
        .await?
        .iter()
        .filter_map(|r| {
            let t = r.fields.u32_of("Tomestones@as(raw)");
            (1..=3)
                .contains(&t)
                .then(|| (t, r.fields.u32_of("Item@as(raw)")))
        })
        .collect();
    let tribes: Vec<u32> = api
        .sheet("BeastTribe", "CurrencyItem@as(raw)")
        .await?
        .iter()
        .map(|r| r.fields.u32_of("CurrencyItem@as(raw)"))
        .filter(|&i| i != 0)
        .collect();
    let mut category_of: HashMap<u32, Category> = HashMap::new();
    for (ids, cat) in [
        (SCRIPS, Category::Assignats),
        (PVP, Category::Jcj),
        (HUNT, Category::Chasse),
        (OTHER, Category::Autres),
    ] {
        for &i in ids {
            category_of.insert(i, cat);
        }
    }
    let mut tome_ids: Vec<u32> = tomestones.values().copied().collect();
    tome_ids.sort_unstable();
    for i in &tome_ids {
        category_of.insert(*i, Category::Memoquartz);
    }
    for i in &tribes {
        category_of.insert(*i, Category::TribusAlliees);
    }
    for (_, seal) in GC_SEALS {
        category_of.insert(seal, Category::GrandeCompagnie);
    }
    let currency_ids: HashSet<u32> = category_of.keys().copied().collect();

    report("Marchands");
    let npc_data: HashMap<u32, Vec<u32>> = api
        .sheet("ENpcBase", "ENpcData@as(raw)")
        .await?
        .into_iter()
        .filter_map(|r| {
            let v = nonzero(r.fields.u32s_of("ENpcData@as(raw)"));
            (!v.is_empty()).then_some((r.row_id, v))
        })
        .collect();

    report("Liens marchands → boutiques");
    let mut links = Links {
        npc_data,
        ..Default::default()
    };
    for r in api.sheet("TopicSelect", "Shop@as(raw)").await? {
        links
            .topic_select
            .insert(r.row_id, nonzero(r.fields.u32s_of("Shop@as(raw)")));
    }
    for r in api.sheet("PreHandler", "Target@as(raw)").await? {
        links
            .pre_handler
            .insert(r.row_id, r.fields.u32_of("Target@as(raw)"));
    }
    for r in api.sheet("InclusionShop", "Category@as(raw)").await? {
        links
            .inclusion_shop
            .insert(r.row_id, nonzero(r.fields.u32s_of("Category@as(raw)")));
    }
    for r in api
        .sheet("InclusionShopCategory", "InclusionShopSeries@as(raw)")
        .await?
    {
        links
            .inclusion_category
            .insert(r.row_id, r.fields.u32_of("InclusionShopSeries@as(raw)"));
    }
    for r in api
        .sheet("InclusionShopSeries", "SpecialShop@as(raw)")
        .await?
    {
        links
            .inclusion_series
            .entry(r.row_id)
            .or_default()
            .push(r.fields.u32_of("SpecialShop@as(raw)"));
    }
    for r in api.sheet("CustomTalk", "Script[].ScriptArg").await? {
        let args: Vec<u32> = r
            .fields
            .get("Script")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|s| s.u32_of("ScriptArg"))
                    .filter(|&x| x != 0)
                    .collect()
            })
            .unwrap_or_default();
        if !args.is_empty() {
            links.custom_talk.insert(r.row_id, args);
        }
    }
    for r in api.sheet("GCShop", "GrandCompany@as(raw)").await? {
        links
            .gc_shop
            .insert(r.row_id, r.fields.u32_of("GrandCompany@as(raw)"));
    }

    report("Échanges contre des sceaux de grande compagnie");
    let gc_category: HashMap<u32, u32> = api
        .sheet("GCScripShopCategory", "GrandCompany@as(raw)")
        .await?
        .iter()
        .map(|r| (r.row_id, r.fields.u32_of("GrandCompany@as(raw)")))
        .collect();
    let gc_vendors = links.gc_vendors();
    let mut raw = special_shop_offers(&shops, &links, &tomestones, &currency_ids, &marketable);
    for r in api
        .sheet(
            "GCScripShopItem",
            "Item@as(raw),CostGCSeals,RequiredGrandCompanyRank@as(raw)",
        )
        .await?
    {
        let item = r.fields.u32_of("Item@as(raw)");
        let cost = r.fields.u32_of("CostGCSeals");
        let gc = gc_category.get(&r.row_id).copied().unwrap_or(0);
        let Some(&(_, seal)) = GC_SEALS.iter().find(|(g, _)| *g == gc) else {
            continue;
        };
        if item == 0 || cost == 0 || !marketable.contains(&item) {
            continue;
        }
        let rank = r.fields.u32_of("RequiredGrandCompanyRank@as(raw)");
        raw.push(RawOffer {
            currency: seal,
            item,
            quantity: 1,
            cost,
            hq: false,
            shop_name: "Intendant de grande compagnie".into(),
            vendors: gc_vendors.get(&gc).cloned().unwrap_or_default(),
            quest: 0,
            achievement: 0,
            gc_rank: (rank > 1).then_some((gc, rank)),
        });
    }
    let raw = merge(raw);

    report("Marchands PNJ (achat en gils)");
    // Boutique → [(objet, quête requise)]
    let mut gil_shops: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
    for r in api
        .sheet("GilShopItem", "Item@as(raw),QuestRequired@as(raw)")
        .await?
    {
        let item = r.fields.u32_of("Item@as(raw)");
        if item != 0 && marketable.contains(&item) {
            let quest = r
                .fields
                .u32s_of("QuestRequired@as(raw)")
                .into_iter()
                .find(|&q| q != 0);
            gil_shops
                .entry(r.row_id)
                .or_default()
                .push((item, quest.unwrap_or(0)));
        }
    }
    let gil_ids: HashSet<u32> = gil_shops.keys().copied().collect();
    let gil_vendors = links.shop_vendors(&gil_ids);
    let raw_gil = gil_shop_items(&gil_shops, &gil_vendors);

    report("Livraisons expertes");
    let seals_by_level: HashMap<u32, u32> = api
        .sheet("GCSupplyDutyReward", "SealsExpertDelivery")
        .await?
        .iter()
        .map(|r| (r.row_id, r.fields.u32_of("SealsExpertDelivery")))
        .collect();
    let mut deliveries: Vec<Delivery> = api
        .search(
            "Item",
            "+Rarity>=2 +Rarity<=3 +EquipSlotCategory>=1",
            "LevelItem@as(raw)",
        )
        .await?
        .iter()
        .filter(|r| marketable.contains(&r.row_id))
        .filter_map(|r| {
            let item_level = r.fields.u32_of("LevelItem@as(raw)");
            let seals = seals_by_level.get(&item_level).copied().unwrap_or(0);
            (seals > 0).then_some(Delivery {
                item: r.row_id,
                item_level,
                seals,
            })
        })
        .collect();
    deliveries.sort_by_key(|d| d.item);
    deliveries.dedup_by_key(|d| d.item);

    report("Missions de servants");
    let thresholds: HashMap<u32, Value> = api
        .sheet(
            "RetainerTaskParameter",
            "ItemLevelDoW,PerceptionDoL,PerceptionFSH",
        )
        .await?
        .into_iter()
        .map(|r| (r.row_id, r.fields))
        .collect();
    let tasks: Vec<Row> = api
        .sheet(
            "RetainerTask",
            "ClassJobCategory@as(raw),RetainerLevel,RequiredItemLevel,RequiredGathering,\
             Task@as(raw),IsRandom,MaxTimemin,VentureCost,RetainerTaskParameter@as(raw)",
        )
        .await?
        .into_iter()
        .filter(|r| {
            let f = &r.fields;
            !f.get("IsRandom").and_then(Value::as_bool).unwrap_or(true)
                && f.u32_of("MaxTimemin") == 60
                && f.u32_of("VentureCost") == 1
        })
        .collect();
    let task_ids: Vec<u32> = tasks
        .iter()
        .map(|t| t.fields.u32_of("Task@as(raw)"))
        .collect();
    let normal: HashMap<u32, Value> = api
        .rows("RetainerTaskNormal", &task_ids, "Item@as(raw),Quantity")
        .await?
        .into_iter()
        .map(|r| (r.row_id, r.fields))
        .collect();
    let mut ventures = Vec::new();
    for t in &tasks {
        let f = &t.fields;
        let (class, column, stat) = match f.u32_of("ClassJobCategory@as(raw)") {
            34 => (VentureClass::Combat, "ItemLevelDoW", "RequiredItemLevel"),
            17 => (VentureClass::Mineur, "PerceptionDoL", "RequiredGathering"),
            18 => (
                VentureClass::Botaniste,
                "PerceptionDoL",
                "RequiredGathering",
            ),
            19 => (VentureClass::Pecheur, "PerceptionFSH", "RequiredGathering"),
            _ => continue,
        };
        let Some(task) = normal.get(&f.u32_of("Task@as(raw)")) else {
            continue;
        };
        let item = task.u32_of("Item@as(raw)");
        if item == 0 || !marketable.contains(&item) {
            continue;
        }
        ventures.push(Venture {
            item,
            class,
            retainer_level: f.u32_of("RetainerLevel"),
            required_stat: f.u32_of(stat),
            thresholds: thresholds
                .get(&f.u32_of("RetainerTaskParameter@as(raw)"))
                .map(|p| p.u32s_of(column))
                .unwrap_or_default(),
            quantities: task.u32s_of("Quantity"),
        });
    }

    report("Désynthèse");
    let desynths = match http
        .get_json::<HashMap<String, Vec<u32>>>(DESYNTH_URL)
        .await
    {
        Ok(raw) => invert_desynth(&raw, &marketable),
        Err(e) => {
            // Source externe : son absence ne doit pas bloquer le reste.
            tracing::warn!(error = %e, "désynthèse indisponible");
            Vec::new()
        }
    };

    report("Noms des objets");
    let mut item_ids: Vec<u32> = raw.iter().map(|o| o.item).collect();
    item_ids.extend(currency_ids.iter().copied());
    item_ids.extend(raw_gil.iter().map(|g| g.item));
    item_ids.extend(deliveries.iter().map(|d| d.item));
    item_ids.extend(ventures.iter().map(|v| v.item));
    item_ids.extend(
        desynths
            .iter()
            .flat_map(|d| std::iter::once(d.item).chain(d.results.iter().copied())),
    );
    item_ids.sort_unstable();
    item_ids.dedup();
    let mut npc_prices: HashMap<u32, u32> = HashMap::new();
    let items: HashMap<u32, ItemMeta> = api
        .rows("Item", &item_ids, "Name,Icon,PriceMid")
        .await?
        .into_iter()
        .map(|r| {
            npc_prices.insert(r.row_id, r.fields.u32_of("PriceMid"));
            let meta = ItemMeta {
                name: clean_name(&r.fields.str_of("Name")),
                icon: icon_path(&r.fields),
            };
            (r.row_id, meta)
        })
        .collect();

    report("Noms des marchands");
    let mut npc_ids: Vec<u32> = raw.iter().flat_map(|o| o.vendors.iter().copied()).collect();
    npc_ids.extend(raw_gil.iter().flat_map(|g| g.vendors.iter().copied()));
    npc_ids.sort_unstable();
    npc_ids.dedup();
    let npc_names: HashMap<u32, String> = api
        .rows("ENpcResident", &npc_ids, "Singular")
        .await?
        .into_iter()
        .map(|r| (r.row_id, clean_name(&r.fields.str_of("Singular"))))
        .collect();

    report("Conditions de déblocage");
    let names_of = |rows: Vec<Row>| -> HashMap<u32, String> {
        rows.into_iter()
            .map(|r| (r.row_id, clean_name(&r.fields.str_of("Name"))))
            .collect()
    };
    let mut quest_ids: Vec<u32> = raw
        .iter()
        .map(|o| o.quest)
        .chain(raw_gil.iter().map(|g| g.quest))
        .filter(|&q| q != 0)
        .collect();
    quest_ids.sort_unstable();
    quest_ids.dedup();
    let quests = names_of(api.rows("Quest", &quest_ids, "Name").await?);
    let mut ach_ids: Vec<u32> = raw
        .iter()
        .map(|o| o.achievement)
        .filter(|&a| a != 0)
        .collect();
    ach_ids.sort_unstable();
    ach_ids.dedup();
    let achievements = names_of(api.rows("Achievement", &ach_ids, "Name").await?);
    let mut gc_ranks: HashMap<(u32, u32), String> = HashMap::new();
    for (gc, sheet) in [
        (1, "GCRankLimsaMaleText"),
        (2, "GCRankGridaniaMaleText"),
        (3, "GCRankUldahMaleText"),
    ] {
        for r in api.sheet(sheet, "Singular").await? {
            gc_ranks.insert((gc, r.row_id), clean_name(&r.fields.str_of("Singular")));
        }
    }

    let offers: Vec<Offer> = raw
        .into_iter()
        .map(|o| {
            let tribe = category_of.get(&o.currency) == Some(&Category::TribusAlliees);
            let reason = if o.quest != 0 {
                quests.get(&o.quest).map(|q| format!("Quête : {q}"))
            } else if o.achievement != 0 {
                achievements
                    .get(&o.achievement)
                    .map(|a| format!("Haut fait : {a}"))
            } else if let Some(key) = o.gc_rank {
                gc_ranks.get(&key).map(|r| format!("Grade requis : {r}"))
            } else if tribe {
                Some("Rang de réputation requis".into())
            } else {
                None
            };
            let locked = o.quest != 0 || o.achievement != 0 || o.gc_rank.is_some() || tribe;
            let mut vendors: Vec<String> = o
                .vendors
                .iter()
                .filter_map(|n| npc_names.get(n))
                .filter(|n| !n.is_empty())
                .cloned()
                .collect();
            vendors.sort();
            vendors.dedup();
            Offer {
                currency: o.currency,
                item: o.item,
                quantity: o.quantity,
                cost: o.cost,
                hq: o.hq,
                shops: o
                    .shop_name
                    .split(" · ")
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect(),
                vendors,
                unlock: locked.then_some(Unlock { reason }),
            }
        })
        .collect();

    let vendor_names = |ids: &BTreeSet<u32>| -> Vec<String> {
        let mut v: Vec<String> = ids
            .iter()
            .filter_map(|n| npc_names.get(n))
            .filter(|n| !n.is_empty())
            .cloned()
            .collect();
        v.sort();
        v.dedup();
        v
    };
    let gil_offers: Vec<GilOffer> = raw_gil
        .iter()
        .filter_map(|g| {
            let price = npc_prices.get(&g.item).copied().unwrap_or(0);
            (price > 0).then(|| GilOffer {
                item: g.item,
                price,
                vendors: vendor_names(&g.vendors),
                unlock: (g.quest != 0).then(|| Unlock {
                    reason: quests.get(&g.quest).map(|q| format!("Quête : {q}")),
                }),
            })
        })
        .collect();

    let mut currencies: Vec<Currency> = category_of
        .iter()
        .filter_map(|(&id, &category)| {
            let mut distinct: Vec<u32> = offers
                .iter()
                .filter(|o| o.currency == id)
                .map(|o| o.item)
                .collect();
            distinct.sort_unstable();
            distinct.dedup();
            let meta = items.get(&id)?;
            (!distinct.is_empty()).then(|| Currency {
                item_id: id,
                name: meta.name.clone(),
                icon: meta.icon.clone(),
                category,
                item_count: distinct.len(),
            })
        })
        .collect();
    currencies.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(Dataset {
        version: DATASET_VERSION,
        built_at_ms: now_ms(),
        currencies,
        offers,
        gil_offers,
        deliveries,
        ventures,
        desynths,
        items,
    })
}

/// Objet de marchand PNJ, avant nommage.
#[derive(Debug, Clone, PartialEq)]
pub struct RawGilItem {
    pub item: u32,
    pub vendors: BTreeSet<u32>,
    /// Quête requise, 0 si l'objet est libre chez au moins un marchand.
    pub quest: u32,
}

/// Regroupe par objet les boutiques en gils ouvertes par au moins un marchand.
pub fn gil_shop_items(
    shops: &HashMap<u32, Vec<(u32, u32)>>,
    vendors: &HashMap<u32, BTreeSet<u32>>,
) -> Vec<RawGilItem> {
    let mut by_item: HashMap<u32, RawGilItem> = HashMap::new();
    for (shop, items) in shops {
        let Some(shop_vendors) = vendors.get(shop) else {
            continue;
        };
        for &(item, quest) in items {
            let e = by_item.entry(item).or_insert_with(|| RawGilItem {
                item,
                vendors: BTreeSet::new(),
                quest,
            });
            e.vendors.extend(shop_vendors);
            if quest == 0 {
                e.quest = 0;
            }
        }
    }
    let mut out: Vec<RawGilItem> = by_item.into_values().collect();
    out.sort_by_key(|g| g.item);
    out
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tomes() -> HashMap<u32, u32> {
        HashMap::from([(1, 28), (2, 48), (3, 49)])
    }

    #[test]
    fn cost_indices_are_mapped_per_shop_type() {
        let t = tomes();
        assert_eq!(cost_item(16, 2, &t), 33913, "assignat mauve d'artisan");
        assert_eq!(cost_item(16, 7, &t), 41785, "assignat orange de récolteur");
        assert_eq!(cost_item(16, 1, &t), 28, "poétique");
        assert_eq!(cost_item(4, 2, &t), 48, "mémoquartz plafonné du moment");
        assert_eq!(
            cost_item(4, 26533, &t),
            26533,
            "un vrai identifiant passe tel quel"
        );
        assert_eq!(
            cost_item(0, 2, &t),
            2,
            "boutique ordinaire : identifiant d'objet"
        );
    }

    fn shop(id: u32, uct: u32, items: Vec<RawShopItem>) -> RawShop {
        RawShop {
            id,
            name: format!("Boutique {id}"),
            use_currency_type: uct,
            quest: 0,
            items,
        }
    }

    fn sale(item: u32, cost_raw: u32, cost: u32) -> RawShopItem {
        RawShopItem {
            items: vec![item, 0],
            counts: vec![1, 0],
            hq: vec![false, false],
            cost_items: vec![cost_raw, 0, 0],
            cost_counts: vec![cost, 0, 0],
            ..Default::default()
        }
    }

    #[test]
    fn only_reachable_single_currency_marketable_offers_survive() {
        let shops = vec![
            shop(
                1769500,
                16,
                vec![
                    sale(5057, 6, 25), // gardé : orange d'artisan, revendable
                    sale(9999, 6, 25), // écarté : pas revendable
                    RawShopItem {
                        // écarté : deux monnaies
                        cost_items: vec![6, 1, 0],
                        cost_counts: vec![25, 100, 0],
                        ..sale(5058, 6, 25)
                    },
                ],
            ),
            shop(1769501, 16, vec![sale(5059, 6, 10)]), // écarté : aucun marchand
        ];
        let links = Links {
            // Le marchand passe par un TopicSelect puis un PreHandler.
            npc_data: HashMap::from([(1000001, vec![3276800])]),
            topic_select: HashMap::from([(3276800, vec![3538944])]),
            pre_handler: HashMap::from([(3538944, 1769500)]),
            ..Default::default()
        };
        let currencies = HashSet::from([41784]);
        let marketable = HashSet::from([5057, 5058, 5059]);
        let offers = special_shop_offers(&shops, &links, &tomes(), &currencies, &marketable);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            (offers[0].item, offers[0].currency, offers[0].cost),
            (5057, 41784, 25)
        );
        assert_eq!(offers[0].vendors, BTreeSet::from([1000001]));
    }

    #[test]
    fn inclusion_shops_are_followed() {
        let shops = vec![shop(1770000, 0, vec![sale(5057, 26533, 40)])];
        let links = Links {
            npc_data: HashMap::from([(1000002, vec![3801088])]),
            inclusion_shop: HashMap::from([(3801088, vec![7])]),
            inclusion_category: HashMap::from([(7, 3)]),
            inclusion_series: HashMap::from([(3, vec![1770000])]),
            ..Default::default()
        };
        let offers = special_shop_offers(
            &shops,
            &links,
            &tomes(),
            &HashSet::from([26533]),
            &HashSet::from([5057]),
        );
        assert_eq!(offers.len(), 1);
    }

    #[test]
    fn merge_keeps_an_offer_unlocked_if_free_anywhere() {
        let base = RawOffer {
            currency: 28,
            item: 5057,
            quantity: 1,
            cost: 10,
            hq: false,
            shop_name: "A".into(),
            vendors: BTreeSet::from([1]),
            quest: 66000,
            achievement: 0,
            gc_rank: None,
        };
        let free = RawOffer {
            shop_name: "B".into(),
            vendors: BTreeSet::from([2]),
            quest: 0,
            ..base.clone()
        };
        let merged = merge(vec![base, free]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].quest, 0);
        assert_eq!(merged[0].vendors, BTreeSet::from([1, 2]));
        assert_eq!(merged[0].shop_name, "A · B");
    }

    #[test]
    fn gil_items_merge_vendors_and_prefer_free_shops() {
        let shops = HashMap::from([
            (262144, vec![(5057, 0), (5058, 66000)]),
            (262145, vec![(5057, 67000)]),
            (262146, vec![(5059, 0)]), // aucun marchand : écarté
        ]);
        let vendors = HashMap::from([(262144, BTreeSet::from([1])), (262145, BTreeSet::from([2]))]);
        let items = gil_shop_items(&shops, &vendors);
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0],
            RawGilItem {
                item: 5057,
                vendors: BTreeSet::from([1, 2]),
                quest: 0
            }
        );
        assert_eq!(items[1].quest, 66000);
    }

    #[test]
    fn desynth_is_inverted_per_marketable_source() {
        let raw = HashMap::from([
            ("13".to_string(), vec![100, 200]),
            ("26730".to_string(), vec![100]),
            ("bad".to_string(), vec![100]),
        ]);
        let d = invert_desynth(&raw, &HashSet::from([100]));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].item, 100);
        assert_eq!(d[0].results, vec![13, 26730]);
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(
            clean_name("\u{e0be} Entre rêve et réalité"),
            "Entre rêve et réalité"
        );
        assert_eq!(
            clean_name("comptable des assignats"),
            "Comptable des assignats"
        );
        assert_eq!(clean_name(""), "");
    }
}
