# FFXIV Trader

Desktop app (macOS, Linux, Windows) that finds Final Fantasy XIV market board
arbitrage: items cheaper on another world or data center than on your home
world, that your home market will actually absorb within a few days. It ranks
opportunities by expected gil per day, not by raw price gap.

Status: **M4** — settings, item search, item detail, full scan with drill-down
and the profit math. Watchlist (M5) and travel grouping/charts/CI (M6) are next.
Full spec: [`ffxiv-arbitrage-scanner-prompt.md`](ffxiv-arbitrage-scanner-prompt.md).

## The math in 90 seconds

You buy on a visited world and always resell on your home world. A price gap
alone is worthless if nobody buys at home, so every number is expressed in
**units the home market absorbs**.

1. **Taxes.** The buyer pays a fixed 5% on top of the listed price; the seller
   pays 0–5% depending on the retainer's city. So a unit costs
   `listing × 1.05` and nets `sell × (1 − seller tax)`.
2. **Home demand.** From the last 7 days of home sales: units sold per day
   (sum of quantities, not sale events) and a quantity-weighted median price
   with outliers dropped. You must undercut the cheapest credible home listing
   (troll listings under half the median are ignored), so the expected sell
   price is `min(median, floor − 1)`.
3. **What to buy.** Listings must be bought whole. Take source listings
   cheapest first while they stay under breakeven and the total stays within
   what home sells during your sell horizon (`units/day × 3 days` by default).
4. **Rank.** `profit per trip = units × net sale − cost`, and
   `gil per day = profit per unit × min(units/day, units ÷ horizon)`. Results
   are ranked by gil per day, with a confidence badge from home sell rate and
   days of competing supply (HOT, OK, COMPETITIVE, SATURATED, SLOW).
5. **Trips.** One trip collects everything worth buying on a world; per-world
   profit per hour fills the 140 inventory slots most profitable first.

An HQ unit can also be sold into NQ demand; the app picks whichever pays more.
The full scan first prefilters ~17,000 items cheaply with Universalis'
aggregated endpoint, then drills into the top 200 with full listings and
history. All thresholds live in `src-tauri/src/profit.rs`.

## Build and run

Prerequisites: [Rust](https://rustup.rs) (stable), Node.js 20+, pnpm, and the
[Tauri system dependencies](https://tauri.app/start/prerequisites/) for your OS.

```sh
pnpm install
pnpm tauri dev      # run with hot reload
pnpm tauri build    # release bundle for the current platform
```

Checks:

```sh
pnpm typecheck && pnpm lint && pnpm format:check
pnpm rs:lint && pnpm rs:test
# end-to-end against the live APIs (network, ~2 min)
cargo test --manifest-path src-tauri/Cargo.toml live_ -- --ignored --nocapture
```

App data (SQLite database) lives in the OS app-data directory
(`~/Library/Application Support/app.ffxivtrader` on macOS); daily-rotated logs
(7 days) in the OS log directory. Settings → About → "Open log folder".

## Attribution

- Market data from [universalis.app](https://universalis.app).
- Item data from [XIVAPI](https://v2.xivapi.com).
- This app only reads Universalis data; it never uploads anything.

FFXIV Trader is a fan tool, not affiliated with or endorsed by Square Enix.
FINAL FANTASY XIV © SQUARE ENIX CO., LTD.
