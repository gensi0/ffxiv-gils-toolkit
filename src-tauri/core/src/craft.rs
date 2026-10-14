//! Artisanat : recettes (XIVAPI), niveaux des métiers (Lodestone) et coût de
//! revient d'un objet, en achetant ou en fabriquant chaque composant selon ce
//! qui revient le moins cher.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http::Http;
use crate::xivapi::XivApi;
use crate::{Error, Result};

/// Métiers d'artisan, dans l'ordre de `CraftType` et du Lodestone.
pub const JOBS: [&str; 8] = [
    "Menuisier",
    "Forgeron",
    "Armurier",
    "Orfèvre",
    "Tanneur",
    "Couturier",
    "Alchimiste",
    "Cuisinier",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recipe {
    pub id: u32,
    pub result: u32,
    pub amount: u32,
    /// Index dans [`JOBS`].
    pub job: u8,
    pub level: u32,
    /// (objet, quantité), cristaux compris.
    pub ingredients: Vec<(u32, u32)>,
    pub expert: bool,
    pub specialist: bool,
    /// Recette d'un livre de maître.
    pub master_book: bool,
}

fn row_id(v: &Value) -> u32 {
    v.get("value")
        .or_else(|| v.get("row_id"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32
}

/// Ligne de la feuille `Recipe` → recette, ou `None` si incomplète.
pub fn parse_recipe(id: u32, f: &Value) -> Option<Recipe> {
    let result = row_id(f.get("ItemResult")?);
    let job = row_id(f.get("CraftType")?);
    if result == 0 || job as usize >= JOBS.len() {
        return None;
    }
    let ids = f.get("Ingredient")?.as_array()?;
    let counts = f.get("AmountIngredient")?.as_array()?;
    let ingredients: Vec<(u32, u32)> = ids
        .iter()
        .zip(counts)
        .map(|(i, c)| (row_id(i), c.as_u64().unwrap_or(0) as u32))
        .filter(|&(i, c)| i > 0 && c > 0)
        .collect();
    if ingredients.is_empty() {
        return None;
    }
    let level = f
        .pointer("/RecipeLevelTable/fields/ClassJobLevel")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    Some(Recipe {
        id,
        result,
        amount: f
            .get("AmountResult")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1) as u32,
        job: job as u8,
        level,
        ingredients,
        expert: f.get("IsExpert").and_then(Value::as_bool).unwrap_or(false),
        specialist: f
            .get("IsSpecializationRequired")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        master_book: f.get("SecretRecipeBook").map(row_id).unwrap_or(0) > 0,
    })
}

const RECIPE_FIELDS: &str = "ItemResult.value,AmountResult,Ingredient[].value,AmountIngredient,\
CraftType.value,RecipeLevelTable.ClassJobLevel,SecretRecipeBook.value,IsExpert,IsSpecializationRequired";

/// Toutes les recettes du jeu.
pub async fn fetch_recipes(xivapi: &XivApi) -> Result<Vec<Recipe>> {
    let rows = xivapi.sheet("Recipe", RECIPE_FIELDS).await?;
    Ok(rows
        .iter()
        .filter_map(|r| parse_recipe(r.row_id, &r.fields))
        .collect())
}

// ---------- Lodestone ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LodestoneJobs {
    pub character: String,
    /// Niveau par métier, dans l'ordre de [`JOBS`] (0 = jamais monté).
    pub levels: Vec<u32>,
}

/// Identifiant numérique depuis une URL Lodestone ou l'identifiant seul.
pub fn lodestone_id(input: &str) -> Option<String> {
    let input = input.trim();
    let digits = match input.find("/character/") {
        Some(i) => &input[i + "/character/".len()..],
        None => input,
    };
    let id: String = digits.chars().take_while(char::is_ascii_digit).collect();
    (id.len() >= 3).then_some(id)
}

fn text_after<'a>(s: &'a str, marker: &str) -> Option<&'a str> {
    let rest = &s[s.find(marker)? + marker.len()..];
    let rest = &rest[rest.find('>')? + 1..];
    Some(rest[..rest.find('<')?].trim())
}

/// Lit les niveaux des métiers d'artisan dans la page `class_job` (en français).
pub fn parse_lodestone(html: &str) -> Option<LodestoneJobs> {
    let character = text_after(html, "frame__chara__name")
        .unwrap_or_default()
        .to_string();
    let mut levels = vec![0; JOBS.len()];
    let mut found = 0;
    for block in html.split("<li>").skip(1) {
        let (Some(level), Some(name)) = (
            text_after(block, "character__job__level"),
            text_after(block, "character__job__name"),
        ) else {
            continue;
        };
        if let Some(i) = JOBS.iter().position(|j| *j == name) {
            levels[i] = level.parse().unwrap_or(0);
            found += 1;
        }
    }
    (found == JOBS.len()).then_some(LodestoneJobs { character, levels })
}

pub async fn fetch_lodestone(http: &Http, input: &str) -> Result<LodestoneJobs> {
    let id = lodestone_id(input)
        .ok_or_else(|| Error::Api("lien ou identifiant Lodestone invalide".into()))?;
    let url = format!("https://fr.finalfantasyxiv.com/lodestone/character/{id}/class_job/");
    let html = http.get_text(&url).await?;
    parse_lodestone(&html).ok_or_else(|| {
        Error::Api("page Lodestone illisible (personnage privé ou introuvable ?)".into())
    })
}

// ---------- coût de revient ----------

/// Prix d'achat le moins cher d'un objet, taxe d'achat comprise.
#[derive(Debug, Clone, PartialEq)]
pub struct BuyPrice {
    pub unit: f64,
    pub world: String,
    pub world_id: i64,
    pub dc: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum Source {
    Buy {
        world: String,
        world_id: i64,
        dc: String,
    },
    Craft {
        recipe_id: u32,
        job: u8,
        level: u32,
    },
    /// Ni achetable (aucune annonce) ni fabricable.
    Missing,
}

/// Arbre de fabrication : un nœud par composant.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub item: u32,
    pub quantity: f64,
    /// Coût unitaire retenu (achat ou fabrication).
    pub unit_cost: f64,
    pub source: Source,
    pub children: Vec<Node>,
}

pub struct Costing<'a> {
    pub by_result: HashMap<u32, Vec<&'a Recipe>>,
    pub prices: &'a HashMap<u32, BuyPrice>,
    /// Niveau par métier ; une recette au-dessus n'est pas fabricable.
    pub levels: &'a [u32],
    pub allow_expert: bool,
    pub allow_master: bool,
    memo: HashMap<u32, Option<(f64, Option<u32>)>>,
}

impl<'a> Costing<'a> {
    pub fn new(
        recipes: &'a [Recipe],
        prices: &'a HashMap<u32, BuyPrice>,
        levels: &'a [u32],
        allow_expert: bool,
        allow_master: bool,
    ) -> Self {
        let mut by_result: HashMap<u32, Vec<&Recipe>> = HashMap::new();
        for r in recipes {
            by_result.entry(r.result).or_default().push(r);
        }
        Self {
            by_result,
            prices,
            levels,
            allow_expert,
            allow_master,
            memo: HashMap::new(),
        }
    }

    pub fn can_craft(&self, r: &Recipe) -> bool {
        let level = self.levels.get(r.job as usize).copied().unwrap_or(0);
        level >= r.level
            && (self.allow_expert || !r.expert)
            && (self.allow_master || !r.master_book)
    }

    fn recipe(&self, id: u32) -> Option<&'a Recipe> {
        self.by_result
            .values()
            .flatten()
            .copied()
            .find(|r| r.id == id)
    }

    /// Coût de fabrication d'une unité avec cette recette (composants au mieux).
    pub fn craft_cost(&mut self, r: &Recipe, stack: &mut HashSet<u32>) -> Option<f64> {
        let mut total = 0.0;
        for &(item, qty) in &r.ingredients {
            let (unit, _) = self.best(item, stack)?;
            total += unit * f64::from(qty);
        }
        Some(total / f64::from(r.amount))
    }

    /// Coût unitaire le plus bas d'un objet et la recette retenue (`None` = achat).
    pub fn best(&mut self, item: u32, stack: &mut HashSet<u32>) -> Option<(f64, Option<u32>)> {
        if let Some(m) = self.memo.get(&item) {
            return *m;
        }
        if !stack.insert(item) {
            return None; // cycle
        }
        let mut best = self.prices.get(&item).map(|p| (p.unit, None));
        let recipes: Vec<&Recipe> = self
            .by_result
            .get(&item)
            .map(|rs| rs.iter().copied().filter(|r| self.can_craft(r)).collect())
            .unwrap_or_default();
        for r in recipes {
            if let Some(c) = self.craft_cost(r, stack) {
                if best.is_none_or(|(b, _)| c < b) {
                    best = Some((c, Some(r.id)));
                }
            }
        }
        stack.remove(&item);
        self.memo.insert(item, best);
        best
    }

    /// Arbre détaillé de `quantity` unités, selon les choix de [`Self::best`].
    pub fn tree(&mut self, item: u32, quantity: f64, forced: Option<&Recipe>) -> Node {
        let mut stack = HashSet::new();
        let chosen = match forced {
            Some(r) => self.craft_cost(r, &mut stack).map(|c| (c, Some(r.id))),
            None => self.best(item, &mut stack),
        };
        match chosen {
            None => Node {
                item,
                quantity,
                unit_cost: 0.0,
                source: Source::Missing,
                children: Vec::new(),
            },
            Some((unit, None)) => Node {
                item,
                quantity,
                unit_cost: unit,
                source: match self.prices.get(&item) {
                    Some(p) => Source::Buy {
                        world: p.world.clone(),
                        world_id: p.world_id,
                        dc: p.dc.clone(),
                    },
                    None => Source::Missing,
                },
                children: Vec::new(),
            },
            Some((unit, Some(rid))) => {
                let r = forced.or_else(|| self.recipe(rid)).expect("recette connue");
                let crafts = quantity / f64::from(r.amount);
                let children = r
                    .ingredients
                    .iter()
                    .map(|&(i, q)| self.tree(i, crafts * f64::from(q), None))
                    .collect();
                Node {
                    item,
                    quantity,
                    unit_cost: unit,
                    source: Source::Craft {
                        recipe_id: r.id,
                        job: r.job,
                        level: r.level,
                    },
                    children,
                }
            }
        }
    }
}

/// Achats à faire pour un arbre : (objet, serveur) → quantité.
pub fn shopping(node: &Node, out: &mut HashMap<(u32, String), f64>) {
    match &node.source {
        Source::Buy { world, .. } => {
            *out.entry((node.item, world.clone())).or_default() += node.quantity
        }
        Source::Craft { .. } => node.children.iter().for_each(|c| shopping(c, out)),
        Source::Missing => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn recipe(id: u32, result: u32, job: u8, level: u32, ing: &[(u32, u32)]) -> Recipe {
        Recipe {
            id,
            result,
            amount: 1,
            job,
            level,
            ingredients: ing.to_vec(),
            expert: false,
            specialist: false,
            master_book: false,
        }
    }

    fn price(unit: f64, world: &str) -> BuyPrice {
        BuyPrice {
            unit,
            world: world.into(),
            world_id: 1,
            dc: "Chaos".into(),
        }
    }

    #[test]
    fn parses_a_recipe_row() {
        let f = json!({
            "AmountIngredient": [1, 2, 0, 0, 0, 0, 3, 0],
            "AmountResult": 3,
            "CraftType": {"value": 6},
            "Ingredient": [{"value": 5057}, {"value": 4856}, {"value": 0}, {"value": 0},
                           {"value": 0}, {"value": 0}, {"value": 2}, {"value": 0}],
            "IsExpert": false,
            "IsSpecializationRequired": true,
            "ItemResult": {"value": 2347},
            "RecipeLevelTable": {"fields": {"ClassJobLevel": 29}},
            "SecretRecipeBook": {"value": 4}
        });
        let r = parse_recipe(101, &f).unwrap();
        assert_eq!(r.result, 2347);
        assert_eq!(r.amount, 3);
        assert_eq!(JOBS[r.job as usize], "Alchimiste");
        assert_eq!(r.level, 29);
        assert_eq!(r.ingredients, vec![(5057, 1), (4856, 2), (2, 3)]);
        assert!(r.specialist && r.master_book && !r.expert);
    }

    #[test]
    fn crafts_a_component_only_when_cheaper_and_allowed() {
        // 10 = objet final (2 × composant 20 + 1 × 30) ; 20 se fabrique avec 3 × 40.
        let recipes = vec![
            recipe(1, 10, 0, 50, &[(20, 2), (30, 1)]),
            recipe(2, 20, 1, 40, &[(40, 3)]),
        ];
        let prices = HashMap::from([
            (20, price(100.0, "Moogle")),
            (30, price(50.0, "Omega")),
            (40, price(10.0, "Louisoix")),
        ]);
        let levels = [100, 100, 0, 0, 0, 0, 0, 0];
        let mut c = Costing::new(&recipes, &prices, &levels, false, false);
        let mut s = HashSet::new();
        // 20 fabriqué : 30 au lieu de 100 → 2 × 30 + 50 = 110.
        assert_eq!(c.best(10, &mut s), Some((110.0, Some(1))));

        let tree = c.tree(10, 2.0, None);
        let mut buy = HashMap::new();
        shopping(&tree, &mut buy);
        assert_eq!(buy[&(40, "Louisoix".to_string())], 12.0);
        assert_eq!(buy[&(30, "Omega".to_string())], 2.0);

        // Forgeron trop bas : on achète le composant.
        let low = [100, 30, 0, 0, 0, 0, 0, 0];
        let mut c = Costing::new(&recipes, &prices, &low, false, false);
        assert_eq!(c.best(10, &mut HashSet::new()), Some((250.0, Some(1))));
    }

    #[test]
    fn source_json_matches_the_front_end() {
        let s = Source::Buy {
            world: "Omega".into(),
            world_id: 39,
            dc: "Chaos".into(),
        };
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            json!({"kind": "buy", "world": "Omega", "worldId": 39, "dc": "Chaos"})
        );
        let c = Source::Craft {
            recipe_id: 1,
            job: 2,
            level: 3,
        };
        assert_eq!(
            serde_json::to_value(&c).unwrap(),
            json!({"kind": "craft", "recipeId": 1, "job": 2, "level": 3})
        );
    }

    #[test]
    fn missing_ingredient_makes_the_recipe_uncraftable() {
        let recipes = vec![recipe(1, 10, 0, 1, &[(20, 1)])];
        let prices = HashMap::new();
        let levels = [100; 8];
        let mut c = Costing::new(&recipes, &prices, &levels, false, false);
        assert_eq!(c.best(10, &mut HashSet::new()), None);
    }

    #[test]
    fn cycles_do_not_loop() {
        let recipes = vec![
            recipe(1, 10, 0, 1, &[(20, 1)]),
            recipe(2, 20, 0, 1, &[(10, 1)]),
        ];
        let prices = HashMap::from([(20, price(5.0, "Moogle"))]);
        let levels = [100; 8];
        let mut c = Costing::new(&recipes, &prices, &levels, false, false);
        assert_eq!(c.best(10, &mut HashSet::new()), Some((5.0, Some(1))));
    }

    #[test]
    fn expert_and_master_recipes_are_opt_in() {
        let mut r = recipe(1, 10, 0, 1, &[(20, 1)]);
        r.expert = true;
        let recipes = vec![r];
        let prices = HashMap::from([(20, price(5.0, "Moogle"))]);
        let levels = [100; 8];
        let c = Costing::new(&recipes, &prices, &levels, false, true);
        assert!(!c.can_craft(&recipes[0]));
        let c = Costing::new(&recipes, &prices, &levels, true, true);
        assert!(c.can_craft(&recipes[0]));
    }

    #[test]
    fn lodestone_ids_and_page() {
        assert_eq!(
            lodestone_id("https://fr.finalfantasyxiv.com/lodestone/character/44017464/").as_deref(),
            Some("44017464")
        );
        assert_eq!(lodestone_id(" 44017464 ").as_deref(), Some("44017464"));
        assert_eq!(lodestone_id("bonjour"), None);

        let mut html = String::from(r#"<p class="frame__chara__name">Gen Si</p><ul>"#);
        for (i, job) in JOBS.iter().enumerate() {
            let level = if i == 0 {
                "-".to_string()
            } else {
                (60 + i).to_string()
            };
            html.push_str(&format!(
                "<li><div class=\"character__job__level\">{level}</div>\n<div class=\"character__job__name js__tooltip\"\n data-tooltip=\"{job}\"\n>{job}</div></li>"
            ));
        }
        let jobs = parse_lodestone(&html).unwrap();
        assert_eq!(jobs.character, "Gen Si");
        assert_eq!(jobs.levels, vec![0, 61, 62, 63, 64, 65, 66, 67]);
    }

    #[test]
    #[ignore = "réseau : cargo test -p spender-core -- --ignored"]
    fn live_lodestone() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let jobs = rt
            .block_on(fetch_lodestone(&Http::new(), "44017464"))
            .unwrap();
        println!("{jobs:?}");
        assert_eq!(jobs.levels.len(), 8);
    }

    #[test]
    #[ignore = "réseau"]
    fn live_recipes() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let recipes = rt
            .block_on(fetch_recipes(&XivApi::new(Http::new())))
            .unwrap();
        println!("{} recettes", recipes.len());
        assert!(recipes.len() > 10_000);
    }
}
