#!/usr/bin/env python3
"""Summarize a recorded spectator feed (mechreplay/1 lines): per-mech accuracy, engagement ranges, deaths, movement."""
import json
import math
import sys
from collections import Counter, defaultdict


def load(path):
    lines = [json.loads(l) for l in open(path) if l.strip()]
    hdr = [x for x in lines if x.get("type") == "header"][-1]
    i = max(k for k, x in enumerate(lines) if x.get("type") == "header")
    frames = [x for x in lines[i:] if "m" in x and "t" in x]
    return hdr, frames


def main(path):
    hdr, fr = load(path)
    bots = hdr["bots"]
    names = {m["id"]: m["name"] for m in hdr["mechs"]}
    kind = {m["id"]: bots[m["id"]].replace("house:", "") for m in hdr["mechs"]}
    pos = {}
    last_seen = {}
    fires = Counter()
    hits = Counter()
    hit_d = defaultdict(list)
    dmg = Counter()
    taken = Counter()
    speeds = defaultdict(list)
    zs = defaultdict(list)
    killer = {}
    prev = {}
    weapons_at = defaultdict(set)
    first_t = fr[0]["t"]
    for x in fr:
        t = x["t"]
        cur = {}
        for m in x["m"]:
            mid = m[0]
            cur[mid] = m
            last_seen[mid] = t
            if mid in prev:
                p = prev[mid]
                dt = (t - p[1]) / hdr["tick_hz"]
                if dt > 0:
                    speeds[mid].append(math.hypot(m[1] - p[0][1], m[2] - p[0][2]) / dt)
            prev[mid] = (m, t)
            zs[mid].append(m[3])
            for s in (11, 12):
                if m[s] >= 0:
                    weapons_at[mid].add(hdr["weapon_kinds"][m[s]])
        for e in x.get("ev", []):
            if e["e"] == "fire" and e.get("w") == "gun":
                fires[e["id"]] += 1
            if e["e"] == "hit" and e.get("dst") is not None and e["dst"] < 16:
                hits[(e["src"], e.get("w"))] += 1
                dmg[e["src"]] += e["amt"]
                taken[e["dst"]] += e["amt"]
                a, b = cur.get(e["src"]), cur.get(e["dst"])
                if a and b:
                    hit_d[(e["src"], e.get("w"))].append(math.dist(a[1:4], b[1:4]))
            if e["e"] == "death" and e.get("id", 99) < 16:
                killer[e["id"]] = (t, e.get("by"))
    tend = fr[-1]["t"]
    print(f"frames {len(fr)}  t {first_t}..{tend} ticks")
    print(f"{'id':>2} {'name':>10} {'kind':>22} {'died':>6} {'gunfire':>7} {'gunhit':>6} {'acc':>5} {'dmg':>6} {'taken':>6} {'med_d':>6} {'spd':>5} {'maxz':>5} weapons")
    for mid in sorted(names):
        died = last_seen.get(mid, 0)
        died_s = f"{died/hdr['tick_hz']:.0f}" if died < tend else "alive"
        gh = hits[(mid, "gun")]
        acc = gh / fires[mid] if fires[mid] else 0
        ds = sorted(d for (s, w), v in hit_d.items() if s == mid for d in v)
        med = ds[len(ds) // 2] if ds else 0
        sp = speeds[mid]
        print(
            f"{mid:>2} {names[mid][:10]:>10} {kind[mid][:22]:>22} {died_s:>6} {fires[mid]:>7} {gh:>6} {acc:>5.2f} {dmg[mid]:>6.0f} {taken[mid]:>6.0f} {med:>6.0f} {sum(sp)/max(1,len(sp)):>5.1f} {max(zs[mid]):>5.1f} {sorted(weapons_at[mid])}"
        )
    print("killers:", {k: v for k, v in killer.items()})


if __name__ == "__main__":
    main(sys.argv[1])
