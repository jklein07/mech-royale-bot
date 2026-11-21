# mech-royale-bot

A bot for the Builders Buildathon **Mech Royale** track: it pilots one mech in a 16-mech
battle royale and speaks the `mechfight/1` JSON-lines protocol on stdin/stdout.

Team: *fortnite pirate token*.

## Layout

| Path | What |
| --- | --- |
| `Dockerfile` | Builds the bot (Rust, no crates) into a small Debian image. |
| `src/main.rs` | stdin/stdout loop; always answers the newest observation. |
| `src/json.rs` | Minimal JSON parser/formatter (no dependencies). |
| `src/proto.rs` | Typed config, observations and actions. |
| `src/world.rs` | Map model from `/arena/v2` (terrain heightmap + rasterized buildings/bridges), line of sight. |
| `src/nav.rs` | 2 m navigation grid and A* path planning. |
| `src/bot.rs` | Strategy: zone rotation, looting, cover, target selection, lead aiming, weapon control. |

## Build and check

```sh
docker build -t my-bot .
python3 check/check.py --map-dir /arena -- docker run --rm -i --network none --cpus 1 --memory 512m \
  --read-only --tmpfs /tmp:rw,size=64m --user 65534:65534 -v "$PWD/arena:/arena:ro" my-bot
```

(`check/check.py` and `arena/` come from the organizers' `mechfight-kit`, which is not included here.)

## Credits

Written from scratch for this event against the organizers' `protocol.md`. The stdin/stdout
handling follows the behaviour of the kit's Python wrapper (`mechfight.py`: answer only the
newest queued observation, keep coasting on errors). No third-party code is included.

## License

MIT, see `LICENSE`.
