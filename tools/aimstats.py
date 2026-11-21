#!/usr/bin/env python3
"""Per-shooter gun accuracy split by intended target (ours vs others) from recorded spectator feeds."""
import json, math, sys, glob
from collections import Counter

def run(paths):
    tot = Counter()
    for path in paths:
        try:
            lines = [json.loads(l) for l in open(path) if l.strip()]
        except Exception:
            continue
        hs = [k for k, x in enumerate(lines) if x.get("type") == "header"]
        if not hs:
            continue
        hdr = lines[hs[-1]]
        fr = [x for x in lines[hs[-1]:] if "m" in x and "t" in x]
        ours = [m["id"] for m in hdr["mechs"] if hdr["bots"][m["id"]] in ("human", "") or "fpt" in m["name"]]
        if not ours:
            continue
        me = ours[0]
        pending = {}  # shooter -> list of intended targets for unresolved shots
        for x in fr:
            cur = {m[0]: m for m in x["m"]}
            for e in x.get("ev", []):
                if e["e"] == "fire" and e.get("w") == "gun" and e["id"] in cur:
                    s = cur[e["id"]]
                    best = None
                    for mid, m in cur.items():
                        if mid == s[0]:
                            continue
                        dx, dy = m[1] - s[1], m[2] - s[2]
                        d = math.hypot(dx, dy)
                        if d < 1 or d > 260:
                            continue
                        ang = abs((math.atan2(dy, dx) - s[4] + math.pi) % (2 * math.pi) - math.pi)
                        if ang < 0.08 and (best is None or d < best[1]):
                            best = (mid, d)
                    if best:
                        who = "me" if best[0] == me else "other"
                        src = "me" if s[0] == me else "bot"
                        tot[(src, who, "fire")] += 1
                        db = "near" if best[1] < 60 else "far"
                        tot[(src, who, db, "fire")] += 1
                        pending.setdefault(s[0], []).append((best[0], db))
                if e["e"] == "hit" and e.get("w") == "gun" and e.get("dst") is not None and e["dst"] < 16 and e.get("src") in pending:
                    lst = pending[e["src"]]
                    for i, (tg, db) in enumerate(lst):
                        if tg == e["dst"]:
                            who = "me" if tg == me else "other"
                            src = "me" if e["src"] == me else "bot"
                            tot[(src, who, "hit")] += 1
                            tot[(src, who, db, "hit")] += 1
                            del lst[i]
                            break
    for src in ("bot", "me"):
        for who in ("me", "other"):
            f, h = tot[(src, who, "fire")], tot[(src, who, "hit")]
            if f:
                extra = "  ".join(f"{db}: {tot[(src, who, db, 'hit')]}/{tot[(src, who, db, 'fire')]}={tot[(src, who, db, 'hit')]/max(1,tot[(src, who, db, 'fire')]):.2f}" for db in ("near", "far"))
                print(f"shooter {src:>3} -> target {who:>5}: {h}/{f} = {h/f:.2f}   {extra}")

if __name__ == "__main__":
    run(sys.argv[1:] or sorted(glob.glob("runs/live/*.live.jsonl")))
