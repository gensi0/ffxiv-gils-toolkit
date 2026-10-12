# FFXIV Trader

Desktop app (macOS, Linux, Windows) that finds Final Fantasy XIV market board
arbitrage: items cheaper on another world or data center than on your home
world, that your home market will actually absorb within a few days. It ranks
opportunities by expected gil per day, not by raw price gap.

Status: **M1** — app shell, settings, home-world picker, SQLite storage.
Scanning and the profit math arrive in M3–M4. Full spec:
[`ffxiv-arbitrage-scanner-prompt.md`](ffxiv-arbitrage-scanner-prompt.md).

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
