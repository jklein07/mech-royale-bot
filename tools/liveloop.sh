#!/bin/bash
# Usage: tools/liveloop.sh TAG BOT_BINARY N  -- plays N private live matches one at a time, logs "TAG place kills"
cd "$(dirname "$0")/.."
TAG=$1; BOT=$2; N=${3:-10}
MAP=../downloads/mech-royale/kit/mechfight-kit/arena
for i in $(seq 1 "$N"); do
  out=$(timeout 900 python3 tools/live.py --bot "$BOT" --map "$MAP" --out runs/live --spectate 2>&1)
  line=$(echo "$out" | grep -m1 "^end ")
  place=$(echo "$line" | sed -E "s/.*'place': ([0-9]+).*/\1/")
  kills=$(echo "$line" | sed -E "s/.*'kills': ([0-9]+).*/\1/")
  echo "$(date +%H:%M) $TAG place=${place:-NA} kills=${kills:-NA}" >> runs/live_results.txt
  sleep 3
done
