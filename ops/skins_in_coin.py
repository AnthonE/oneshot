#!/usr/bin/env python3
"""Move the shop from PROS to oneshot's own coin, on the day the coin lands.

    python3 ops/skins_in_coin.py <the coin's mint>            # prints the new economy block
    python3 ops/skins_in_coin.py <the coin's mint> --write    # writes it into elo.json

Elo Pros reads elo.json from this repo, so committing it to main is the switch:
the shop prices in the coin, a sale lands in the same payer wallet (the prize
wallet, which also pays the daily pot), and the site's "Get" link follows the
ticker. Run it only once the coin is on record (Elo Pros'
docs/launch/ONESHOT-COIN.md §5); before then the shop stays in PROS.

The prices (operator, 2026-11-08: skins are paid for in ONESHOT): gear at
10,000 and the skins at 25,000 / 50,000 / 75,000 / 100,000, so a player who
maxes the daily pot (5,000 a day) earns gear in two days and Gilded in twenty.
"""
import json
import re
import sys
from pathlib import Path

ELO = Path(__file__).resolve().parent.parent / "elo.json"
TICKER = "ONESHOT"
SKINS = {"ember": 25_000, "glacier": 50_000, "verdant": 75_000, "gilded": 100_000}
GEAR = 10_000
B58 = re.compile(r"^[1-9A-HJ-NP-Za-km-z]{32,44}$")


def main(argv):
    if not argv or not B58.match(argv[0]):
        print(__doc__)
        return 2
    mint, write = argv[0], "--write" in argv
    doc = json.loads(ELO.read_text())
    eco = doc["economy"]
    eco["coin"] = mint
    eco["currency"] = {"name": TICKER, "symbol": TICKER}
    for it in eco["items"]:
        it["price"] = SKINS.get(it["id"], GEAR)
    if not write:
        print(json.dumps({k: v for k, v in eco.items() if k != "items"}, indent=1))
        for it in eco["items"]:
            print(f"  {it['id']:<11} {it['price']:>7,} {TICKER}")
        print("\nThat's the shop in the coin. To write it: the same command with --write")
        return 0
    ELO.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n")
    print(f"elo.json: the shop is in {TICKER} ({mint}). Commit it to main to switch it.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
