//! Construit le jeu de données réel et affiche le top d'une monnaie.
//!
//! `cargo run -p spender-core --example live -- [serveur] [id monnaie]`

use spender_core::{dataset, http::Http, pricing, universalis::Universalis};

#[tokio::main]
async fn main() -> spender_core::Result<()> {
    tracing_subscriber::fmt().with_env_filter("warn").init();
    let mut args = std::env::args().skip(1);
    let world = args.next().unwrap_or_else(|| "Moogle".into());
    let currency: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(41784);

    let http = Http::new();
    let t = std::time::Instant::now();
    let ds = dataset::build(&http, &|p| eprintln!("[{}/{}] {}", p.done, p.total, p.step)).await?;
    eprintln!("construit en {:.1}s", t.elapsed().as_secs_f32());
    std::fs::write("/tmp/ffxivspender-dataset.json", serde_json::to_vec(&ds)?)?;

    println!(
        "{} monnaies, {} échanges, {} objets",
        ds.currencies.len(),
        ds.offers.len(),
        ds.items.len()
    );
    for c in &ds.currencies {
        let locked = ds
            .offers
            .iter()
            .filter(|o| o.currency == c.item_id && o.unlock.is_some())
            .count();
        println!(
            "  {:?} · {} ({}) : {} objets, {} échanges verrouillés",
            c.category, c.name, c.item_id, c.item_count, locked
        );
    }

    let ids: Vec<u32> = ds
        .offers
        .iter()
        .filter(|o| o.currency == currency)
        .map(|o| o.item)
        .collect();
    let markets = Universalis::new(http).markets(&world, &ids).await?;
    let rows = pricing::price_offers(&ds, currency, &markets);
    let name = &ds.items[&currency].name;
    println!("\nTop {name} sur {world} :");
    for r in rows.iter().take(12) {
        println!(
            "  {:>8} gils/u · {:<45} ×{} pour {:>4} · {:>6.1} ventes/j · {} · {}",
            r.gil_per_currency.map_or("—".into(), |g| format!("{g:.0}")),
            r.name,
            r.quantity,
            r.cost,
            r.daily_velocity.unwrap_or(0.0),
            r.vendors.join(", "),
            r.unlock.as_ref().map_or("libre".into(), |u| u
                .reason
                .clone()
                .unwrap_or("déblocage requis".into())),
        );
    }
    let uni = Universalis::new(Http::new());
    let gil_ids: Vec<u32> = ds.gil_offers.iter().map(|o| o.item).collect();
    let gm = uni.markets(&world, &gil_ids).await?;
    println!(
        "\nRevente PNJ sur {world} ({} objets) :",
        ds.gil_offers.len()
    );
    for r in pricing::price_gil_offers(&ds, &gm, pricing::MARKET_TAX)
        .iter()
        .take(10)
    {
        println!(
            "  {:>9} gils/j · {:<40} PNJ {:>6} → HV {:>8} · {:>5.1}/j · {}",
            r.daily_profit.map_or("—".into(), |g| format!("{g:.0}")),
            r.name,
            r.npc_price,
            r.unit_price.map_or("—".into(), |g| format!("{g:.0}")),
            r.daily_velocity.unwrap_or(0.0),
            r.vendors
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    let seal = 20; // Maelstrom
    let seal_ids: Vec<u32> = ds
        .offers
        .iter()
        .filter(|o| o.currency == seal)
        .map(|o| o.item)
        .collect();
    let sm = uni.markets(&world, &seal_ids).await?;
    let value = pricing::currency_value(&ds, seal, &sm, pricing::MARKET_TAX);
    println!("référence : {value:?}");
    let value = value.map(|v| v.gil);
    let del_ids: Vec<u32> = ds.deliveries.iter().map(|d| d.item).collect();
    let dm = uni.markets(&world, &del_ids).await?;
    println!(
        "\nLivraisons expertes ({} objets), sceau à {:?} gils :",
        ds.deliveries.len(),
        value
    );
    for r in pricing::price_deliveries(&ds, &dm, value).iter().take(10) {
        println!(
            "  {:>6} gils/sceau · {:<45} niv. {:>3} · {:>5} sceaux · achat {:>7} · gain {:>8}",
            r.gil_per_seal.map_or("—".into(), |g| format!("{g:.2}")),
            r.name,
            r.item_level,
            r.seals,
            r.buy_price.map_or("—".into(), |g| format!("{g:.0}")),
            r.profit.map_or("—".into(), |g| format!("{g:.0}")),
        );
    }
    let v_ids: Vec<u32> = ds.ventures.iter().map(|v| v.item).collect();
    let vm = uni.markets(&world, &v_ids).await?;
    let mut vs = pricing::price_ventures(&ds, &vm);
    let best = |v: &pricing::PricedVenture| {
        v.unit_price.unwrap_or(0.0) * f64::from(*v.quantities.last().unwrap_or(&0))
    };
    vs.retain(|v| v.daily_velocity.unwrap_or(0.0) >= 1.0);
    vs.sort_by(|a, b| best(b).total_cmp(&best(a)));
    println!(
        "\nMissions ({} au total), top au palier max :",
        ds.ventures.len()
    );
    for v in vs.iter().take(8) {
        println!(
            "  {:>8.0} gils · {:?} niv.{} · {:<35} ×{:?} · seuils {:?} · {:.0}/j",
            best(v),
            v.class,
            v.retainer_level,
            v.name,
            v.quantities,
            v.thresholds,
            v.daily_velocity.unwrap_or(0.0)
        );
    }

    let mut d_ids: Vec<u32> = ds.desynths.iter().map(|d| d.item).collect();
    d_ids.extend(ds.desynths.iter().flat_map(|d| d.results.iter().copied()));
    d_ids.sort_unstable();
    d_ids.dedup();
    let dm = uni.markets(&world, &d_ids).await?;
    let des = pricing::price_desynths(&ds, &dm, pricing::MARKET_TAX);
    println!("\nDésynthèse ({} objets) :", des.len());
    for d in des
        .iter()
        .filter(|d| d.daily_velocity.unwrap_or(0.0) >= 1.0)
        .take(8)
    {
        println!(
            "  gain max {:>8.0} · {:<35} achat {:>7.0} · {}",
            d.best_gain.unwrap_or(0.0),
            d.name,
            d.buy_price.unwrap_or(0.0),
            d.results
                .iter()
                .map(|r| format!("{} ({:.0})", r.name, r.unit_price.unwrap_or(0.0)))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}
