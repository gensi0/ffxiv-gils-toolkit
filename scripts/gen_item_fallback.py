#!/usr/bin/env python3
"""Builds src-tauri/data/items-fallback.tsv.gz from xivapi/ffxiv-datamining.

The app uses this bundled subset only when XIVAPI is unreachable. Re-run after
a game patch:  python3 scripts/gen_item_fallback.py

Columns: id, name_en, name_fr, name_de, name_ja, icon_id, stack_size, can_be_hq,
level_item, search_category_id, category_group, cat_en, cat_fr, cat_de, cat_ja.
Only marketable items (ItemSearchCategory > 0) are kept.
"""

import csv
import gzip
import io
import pathlib
import urllib.request

BASE = "https://raw.githubusercontent.com/xivapi/ffxiv-datamining/master/csv"
LANGS = ["en", "fr", "de", "ja"]
OUT = pathlib.Path(__file__).resolve().parent.parent / "src-tauri" / "data" / "items-fallback.tsv.gz"


def fetch_rows(lang: str, sheet: str) -> tuple[list[str], list[list[str]]]:
    with urllib.request.urlopen(f"{BASE}/{lang}/{sheet}.csv", timeout=120) as res:
        text = res.read().decode("utf-8-sig")
    reader = csv.reader(io.StringIO(text))
    header = next(reader)
    return header, list(reader)


def column(header: list[str], rows: list[list[str]], key: str, value: str) -> dict[str, str]:
    k, v = header.index(key), header.index(value)
    return {r[k]: r[v] for r in rows if len(r) > max(k, v)}


def main() -> None:
    header, rows = fetch_rows("en", "Item")
    idx = {n: header.index(n) for n in ["#", "Name", "Icon", "StackSize", "CanBeHq", "ItemSearchCategory", "LevelItem"]}
    names = {"en": column(header, rows, "#", "Name")}
    for lang in LANGS[1:]:
        h, r = fetch_rows(lang, "Item")
        names[lang] = column(h, r, "#", "Name")

    cat_header, cat_rows = fetch_rows("en", "ItemSearchCategory")
    cat_group = column(cat_header, cat_rows, "#", "Category")
    cat_names = {}
    for lang in LANGS:
        h, r = fetch_rows(lang, "ItemSearchCategory")
        cat_names[lang] = column(h, r, "#", "Name")

    out = io.StringIO()
    kept = 0
    for r in rows:
        isc = r[idx["ItemSearchCategory"]]
        if not isc.isdigit() or int(isc) == 0:
            continue
        item_id = r[idx["#"]]
        fields = [
            item_id,
            *(names[lang].get(item_id, "") for lang in LANGS),
            r[idx["Icon"]],
            r[idx["StackSize"]],
            "1" if r[idx["CanBeHq"]] == "True" else "0",
            r[idx["LevelItem"]],
            isc,
            cat_group.get(isc, "0"),
            *(cat_names[lang].get(isc, "") for lang in LANGS),
        ]
        out.write("\t".join(f.replace("\t", " ").replace("\n", " ") for f in fields) + "\n")
        kept += 1

    OUT.write_bytes(gzip.compress(out.getvalue().encode("utf-8"), mtime=0))
    print(f"{kept} marketable items -> {OUT} ({OUT.stat().st_size // 1024} KiB)")


if __name__ == "__main__":
    main()
