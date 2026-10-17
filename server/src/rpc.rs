//! Appels du client web : `POST /api/cmd/<nom>` avec les arguments en JSON
//! (mêmes noms que les commandes Tauri, en camelCase). Généré depuis les
//! signatures des commandes.

use serde::Deserialize;
use serde_json::Value;

use crate::app::AppHandle;
use crate::craft::{CraftOptions, CraftState};
use crate::settings::Settings;
use crate::spender::SpenderState;
use crate::{commands, craft, spender, ventes, AppState};

fn args<T: for<'de> Deserialize<'de>>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| format!("arguments invalides : {e}"))
}

fn reply<T: serde::Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

#[allow(clippy::too_many_lines)]
pub async fn dispatch(app: &AppHandle, cmd: &str, input: Value) -> Result<Value, String> {
    match cmd {
        "list_worlds" => reply(
            commands::list_worlds(app.state::<AppState>())
                .await
                .map_err(|e| e.to_string())?,
        ),
        "tax_rates" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                world_id: i64,
            }
            let a: A = args(input)?;
            reply(
                commands::tax_rates(app.state::<AppState>(), a.world_id)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "get_settings" => reply(
            commands::get_settings(app.state::<AppState>())
                .await
                .map_err(|e| e.to_string())?,
        ),
        "save_settings" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                settings: Settings,
            }
            let a: A = args(input)?;
            reply(
                commands::save_settings(app.state::<AppState>(), a.settings)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "app_info" => reply(commands::app_info(app.clone(), app.state::<AppState>())),
        "net_status" => reply(commands::net_status(app.state::<AppState>())),
        "search_items" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                text: String,
            }
            let a: A = args(input)?;
            reply(
                commands::search_items(app.state::<AppState>(), a.text)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "item_detail" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                item_id: u32,
                refresh: bool,
            }
            let a: A = args(input)?;
            reply(
                commands::item_detail(app.state::<AppState>(), a.item_id, a.refresh)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "start_scan" => reply(commands::start_scan(app.clone()).map_err(|e| e.to_string())?),
        "cancel_scan" => reply(commands::cancel_scan(app.state::<AppState>())),
        "scan_progress" => reply(commands::scan_progress(app.state::<AppState>())),
        "list_scans" => reply(
            commands::list_scans(app.state::<AppState>())
                .await
                .map_err(|e| e.to_string())?,
        ),
        "scan_results" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                scan_id: Option<String>,
            }
            let a: A = args(input)?;
            reply(
                commands::scan_results(app.state::<AppState>(), a.scan_id)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "scan_candidates" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                scan_id: String,
            }
            let a: A = args(input)?;
            reply(
                commands::scan_candidates(app.state::<AppState>(), a.scan_id)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "watchlist_list" => reply(
            commands::watchlist_list(app.state::<AppState>())
                .await
                .map_err(|e| e.to_string())?,
        ),
        "watchlist_add" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                item_id: u32,
            }
            let a: A = args(input)?;
            reply(
                commands::watchlist_add(app.state::<AppState>(), a.item_id)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "watchlist_remove" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                item_id: u32,
            }
            let a: A = args(input)?;
            reply(
                commands::watchlist_remove(app.state::<AppState>(), a.item_id)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "watchlist_set_threshold" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                item_id: u32,
                profit: Option<i64>,
            }
            let a: A = args(input)?;
            reply(
                commands::watchlist_set_threshold(app.state::<AppState>(), a.item_id, a.profit)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "ws_status" => reply(commands::ws_status(app.state::<AppState>())),
        "retry_failed_scan" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                scan_id: String,
            }
            let a: A = args(input)?;
            reply(
                commands::retry_failed_scan(app.clone(), a.scan_id)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "market_stats" => reply(
            commands::market_stats(app.state::<AppState>())
                .await
                .map_err(|e| e.to_string())?,
        ),
        "ventes_overview" => reply(
            commands::ventes_overview(app.state::<AppState>())
                .await
                .map_err(|e| e.to_string())?,
        ),
        "ventes_refresh" => reply(
            commands::ventes_refresh(app.clone(), app.state::<AppState>())
                .await
                .map_err(|e| e.to_string())?,
        ),
        "ventes_discover" => reply(
            commands::ventes_discover(app.clone(), app.state::<AppState>())
                .await
                .map_err(|e| e.to_string())?,
        ),
        "ventes_set_price" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                listing_id: String,
                price: i64,
            }
            let a: A = args(input)?;
            reply(
                commands::ventes_set_price(app.state::<AppState>(), a.listing_id, a.price)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "ventes_close" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                listing_id: String,
                sold: bool,
                price: Option<i64>,
            }
            let a: A = args(input)?;
            reply(
                commands::ventes_close(app.state::<AppState>(), a.listing_id, a.sold, a.price)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "ventes_track" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                item_id: u32,
                on: bool,
            }
            let a: A = args(input)?;
            reply(
                commands::ventes_track(app.state::<AppState>(), a.item_id, a.on)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "journal_list" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                since_ms: i64,
            }
            let a: A = args(input)?;
            reply(
                commands::journal_list(app.state::<AppState>(), a.since_ms)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "journal_add_purchase" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                item_id: u32,
                hq: bool,
                quantity: i64,
                unit_cost: f64,
                source_world: Option<String>,
            }
            let a: A = args(input)?;
            reply(
                commands::journal_add_purchase(
                    app.state::<AppState>(),
                    a.item_id,
                    a.hq,
                    a.quantity,
                    a.unit_cost,
                    a.source_world,
                )
                .await
                .map_err(|e| e.to_string())?,
            )
        }
        "journal_delete_purchase" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: i64,
            }
            let a: A = args(input)?;
            reply(
                commands::journal_delete_purchase(app.state::<AppState>(), a.id)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "journal_add_sale" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                item_id: u32,
                hq: bool,
                quantity: i64,
                unit_price: i64,
            }
            let a: A = args(input)?;
            reply(
                commands::journal_add_sale(
                    app.state::<AppState>(),
                    a.item_id,
                    a.hq,
                    a.quantity,
                    a.unit_price,
                )
                .await
                .map_err(|e| e.to_string())?,
            )
        }
        "journal_delete_sale" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: i64,
            }
            let a: A = args(input)?;
            reply(
                commands::journal_delete_sale(app.state::<AppState>(), a.id)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "journal_record_craft" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                item_id: u32,
                quantity: i64,
                ingredients: Vec<ventes::CraftInput>,
            }
            let a: A = args(input)?;
            reply(
                commands::journal_record_craft(
                    app.state::<AppState>(),
                    a.item_id,
                    a.quantity,
                    a.ingredients,
                )
                .await
                .map_err(|e| e.to_string())?,
            )
        }
        "spender_dataset_status" => reply(
            spender::spender_dataset_status(app.state::<SpenderState>())
                .await
                .map_err(|e| e.to_string())?,
        ),
        "spender_build_dataset" => reply(
            spender::spender_build_dataset(app.clone(), app.state::<SpenderState>())
                .await
                .map_err(|e| e.to_string())?,
        ),
        "spender_currencies" => reply(
            spender::spender_currencies(app.state::<SpenderState>())
                .await
                .map_err(|e| e.to_string())?,
        ),
        "spender_tax" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                world: String,
            }
            let a: A = args(input)?;
            reply(
                spender::spender_tax(app.state::<AppState>(), a.world)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "spender_offers" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                currency: u32,
                world: String,
                refresh: bool,
            }
            let a: A = args(input)?;
            reply(
                spender::spender_offers(
                    app.state::<AppState>(),
                    app.state::<SpenderState>(),
                    a.currency,
                    a.world,
                    a.refresh,
                )
                .await
                .map_err(|e| e.to_string())?,
            )
        }
        "spender_gil_offers" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                world: String,
                refresh: bool,
            }
            let a: A = args(input)?;
            reply(
                spender::spender_gil_offers(
                    app.state::<AppState>(),
                    app.state::<SpenderState>(),
                    a.world,
                    a.refresh,
                )
                .await
                .map_err(|e| e.to_string())?,
            )
        }
        "spender_deliveries" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                world: String,
                seal: u32,
                refresh: bool,
            }
            let a: A = args(input)?;
            reply(
                spender::spender_deliveries(
                    app.state::<AppState>(),
                    app.state::<SpenderState>(),
                    a.world,
                    a.seal,
                    a.refresh,
                )
                .await
                .map_err(|e| e.to_string())?,
            )
        }
        "spender_ventures" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                world: String,
                refresh: bool,
            }
            let a: A = args(input)?;
            reply(
                spender::spender_ventures(
                    app.state::<AppState>(),
                    app.state::<SpenderState>(),
                    a.world,
                    a.refresh,
                )
                .await
                .map_err(|e| e.to_string())?,
            )
        }
        "spender_desynths" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                world: String,
                refresh: bool,
            }
            let a: A = args(input)?;
            reply(
                spender::spender_desynths(
                    app.state::<AppState>(),
                    app.state::<SpenderState>(),
                    a.world,
                    a.refresh,
                )
                .await
                .map_err(|e| e.to_string())?,
            )
        }
        "craft_analyze" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                options: CraftOptions,
            }
            let a: A = args(input)?;
            reply(
                craft::craft_analyze(app.clone(), app.state::<CraftState>(), a.options)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "craft_last" => reply(
            craft::craft_last(app.state::<CraftState>())
                .await
                .map_err(|e| e.to_string())?,
        ),
        "craft_lodestone" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                input: String,
            }
            let a: A = args(input)?;
            reply(
                craft::craft_lodestone(app.state::<CraftState>(), a.input)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        other => Err(format!("commande inconnue : {other}")),
    }
}
