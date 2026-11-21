//! mechfight/1 bot entry point: stdin/stdout loop that always answers the newest observation.

use mechbot::bot::{Bot, Params};
use mechbot::json;
use mechbot::nav::Nav;
use mechbot::proto::{Action, Config, Obs};
use mechbot::world::World;
use std::io::{BufRead, Write};
use std::sync::mpsc;
use std::sync::Arc;

fn log(s: &str) {
    eprintln!("{}", s);
}

fn main() {
    let (tx, rx) = mpsc::channel::<Option<String>>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut lock = stdin.lock();
        let mut line = String::new();
        loop {
            line.clear();
            match lock.read_line(&mut line) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(None);
                    return;
                }
                Ok(_) => {
                    if !line.trim().is_empty() {
                        if tx.send(Some(line.clone())).is_err() {
                            return;
                        }
                    }
                }
            }
        }
    });
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut bot: Option<Bot> = None;
    let mut last_good: Action = Action::default();
    let debug = std::env::var("MECHBOT_DEBUG").is_ok();
    loop {
        let first = match rx.recv() {
            Ok(Some(l)) => l,
            _ => return,
        };
        let mut batch = vec![first];
        loop {
            match rx.try_recv() {
                Ok(Some(l)) => batch.push(l),
                Ok(None) => {
                    batch.push(String::new());
                    break;
                }
                Err(_) => break,
            }
        }
        let mut latest: Option<String> = None;
        let mut quit = false;
        for raw in batch {
            if raw.is_empty() {
                quit = true;
                break;
            }
            // cheap type sniffing; full parse only for hello and the newest obs
            if raw.contains("\"type\":\"obs\"") || raw.contains("\"type\": \"obs\"") {
                latest = Some(raw);
                continue;
            }
            let v = match json::parse(&raw) {
                Ok(v) => v,
                Err(_) => continue,
            };
            match v.get("type").s() {
                "hello" => {
                    let cfg = Config::from(v.get("config"));
                    let dir = v.get("map").get("dir").s().to_string();
                    let seed = v.get("seed").f() as u64;
                    let slot = v.get("slot").f() as i32;
                    match World::load(&dir) {
                        Ok(w) => {
                            let nav = Nav::build(&w, cfg.arena_margin.min(10.0));
                            let mut b = Bot::new(cfg, Arc::new(w), nav, seed, slot, Params::default());
                            b.debug = debug;
                            bot = Some(b);
                        }
                        Err(e) => log(&format!("map load failed: {}", e)),
                    }
                    let _ = writeln!(out, "{{\"type\":\"ready\",\"name\":\"fortnite-pirate-mech\"}}");
                    let _ = out.flush();
                }
                "obs" => latest = Some(raw),
                "end" => {
                    quit = true;
                    break;
                }
                _ => {}
            }
        }
        if let Some(raw) = latest {
            let v = match json::parse(&raw) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let tick = v.get("tick").f() as i64;
            let act = match bot.as_mut() {
                Some(b) => {
                    let o = Obs::from(&v);
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| b.act(&o))) {
                        Ok(a) => {
                            last_good = a.clone();
                            a
                        }
                        Err(_) => {
                            log(&format!("act panicked at tick {}", tick));
                            let mut a = Action::default();
                            a.mv = last_good.mv;
                            a.aim = last_good.aim;
                            a.boost = last_good.boost;
                            a
                        }
                    }
                }
                None => Action::default(),
            };
            let _ = writeln!(out, "{}", act.to_json(tick));
            let _ = out.flush();
        }
        if quit {
            return;
        }
    }
}
