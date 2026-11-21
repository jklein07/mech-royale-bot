//! Local match runner. Usage:
//!   sim --hello HELLO.json --map DIR [--mode practice|ffa] [--n 48] [--threads 3] [--seed 1]
//!       [--a "k=v,..."] [--b "k=v,..."] [--mix "bot:4,starter:4,brawler:4,house:4"]
//! Paired runs: the same seeds are played with params A and with params B.

use mechbot::agents::*;
use mechbot::bot::Params;
use mechbot::json;
use mechbot::nav::Nav;
use mechbot::proto::Config;
use mechbot::sim::{Agent, Intel, MatchCfg, Sim};
use mechbot::world::World;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

fn parse_params(s: &str) -> Params {
    let mut p = Params::default();
    for kv in s.split(',') {
        let kv = kv.trim();
        if kv.is_empty() {
            continue;
        }
        let mut it = kv.splitn(2, '=');
        let k = it.next().unwrap();
        let v = it.next().unwrap_or("1");
        let f: f64 = v.parse().unwrap_or(0.0);
        match k {
            "fight_ratio" => p.fight_ratio = f,
            "retreat_hp" => p.retreat_hp = f,
            "hold_frac" => p.hold_frac = f,
            "zone_margin_s" => p.zone_margin_s = f,
            "loot_range" => p.loot_range = f,
            "hunt_npcs" => p.hunt_npcs = f != 0.0,
            "passive" => p.passive = f != 0.0,
            "w_mortar" => p.wpref[1] = f,
            "w_laser" => p.wpref[2] = f,
            "w_lightning" => p.wpref[3] = f,
            _ => eprintln!("unknown param {}", k),
        }
    }
    p
}

struct Res {
    place: i32,
    kills: i32,
    dmg: f64,
    survive_t: f64,
}

fn run_match(
    cfg: &Config,
    world: &Arc<World>,
    nav: &Arc<Nav>,
    mode: &str,
    mix: &str,
    seed: u64,
    params: &Params,
    verbose: bool,
) -> (Res, Vec<(String, i32)>) {
    let n = cfg.mechs;
    let mut rng = mechbot::geom::Rng(seed.wrapping_mul(2862933555777941757).wrapping_add(3037000493) | 1);
    let me = (rng.next() % n as u64) as usize;
    let mut squad = vec![false; n];
    let mut agents: Vec<Box<dyn Agent>> = Vec::new();
    let intel = Rc::new(RefCell::new(Intel::default()));
    let mut rank = vec![0i32; n];
    if mode == "practice" {
        let skills = [
            Skill::Easy,
            Skill::Easy,
            Skill::Easy,
            Skill::Easy,
            Skill::Medium,
            Skill::Medium,
            Skill::Medium,
            Skill::Medium,
            Skill::Hard,
            Skill::Hard,
            Skill::Hard,
            Skill::Hard,
            Skill::Expert,
            Skill::Expert,
            Skill::Expert,
        ];
        let mut k = 0;
        let squad_ids: Vec<i32> = (0..n as i32).filter(|&i| i as usize != me).collect();
        for i in 0..n {
            if i == me {
                agents.push(mk_bot(cfg, world, nav, seed ^ 0xabc, i as i32, params.clone(), "me"));
            } else {
                squad[i] = true;
                rank[i] = k as i32;
                agents.push(Box::new(House::new(i as i32, skills[k], (**nav).clone(), cfg.clone(), world.clone(), seed.wrapping_add(i as u64 * 77), intel.clone(), squad_ids.clone())));
                k += 1;
            }
        }
    } else {
        // ffa: me + mix
        let mut kinds: Vec<String> = Vec::new();
        for part in mix.split(',') {
            let mut it = part.split(':');
            let k = it.next().unwrap_or("bot").to_string();
            let c: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(1);
            for _ in 0..c {
                kinds.push(k.clone());
            }
        }
        while kinds.len() < n - 1 {
            kinds.push("house".into());
        }
        let mut ki = 0;
        for i in 0..n {
            if i == me {
                agents.push(mk_bot(cfg, world, nav, seed ^ 0xabc, i as i32, params.clone(), "me"));
                continue;
            }
            let k = kinds[ki].clone();
            ki += 1;
            let s = seed.wrapping_add(i as u64 * 1013);
            let a: Box<dyn Agent> = match k.as_str() {
                "bot" => mk_bot(cfg, world, nav, s, i as i32, Params::default(), "bot"),
                "starter" => Box::new(Starter { reach: cfg.pickup_radius }),
                "brawler" => Box::new(Brawler::new((**nav).clone(), world.clone(), s)),
                "passive" => {
                    let mut p = Params::default();
                    p.passive = true;
                    mk_bot(cfg, world, nav, s, i as i32, p, "passive")
                }
                _ => {
                    let sk = [Skill::Easy, Skill::Medium, Skill::Hard, Skill::Expert][(s % 4) as usize];
                    // ffa house: everyone is an enemy (no squad ids)
                    Box::new(House::new(i as i32, sk, (**nav).clone(), cfg.clone(), world.clone(), s, Rc::new(RefCell::new(Intel::default())), vec![i as i32]))
                }
            };
            agents.push(a);
        }
    }
    let mc = MatchCfg { seed, squad: squad.clone(), max_t: cfg.duration_s };
    let mut sim = Sim::new(cfg.clone(), world.clone(), nav.clone(), &mc);
    if mode == "practice" {
        sim.intel = Some(intel.clone());
        sim.rank = rank;
    }
    let place = sim.run(&mut agents);
    let m = &sim.mechs[me];
    let surv = if m.alive { sim.t } else { m.death_tick as f64 / cfg.tick_hz };
    if verbose {
        eprintln!(
            "seed {} slot {} place {} kills {} dmg {:.0} t {:.0} cubes {} weapons {:?}",
            seed,
            me,
            place[me],
            m.kills,
            m.dmg_dealt,
            surv,
            m.cubes,
            m.weapons.iter().flatten().map(|w| w.kind.name()).collect::<Vec<_>>()
        );
    }
    let others: Vec<(String, i32)> = (0..n).filter(|&i| i != me).map(|i| (agents[i].name(), place[i])).collect();
    (Res { place: place[me], kills: m.kills, dmg: m.dmg_dealt, survive_t: surv }, others)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str, d: &str| -> String {
        for i in 0..args.len() {
            if args[i] == k && i + 1 < args.len() {
                return args[i + 1].clone();
            }
        }
        d.to_string()
    };
    let hello_path = get("--hello", "runs/hello.json");
    let map = get("--map", "arena");
    let mode = get("--mode", "practice");
    let n: usize = get("--n", "24").parse().unwrap();
    let threads: usize = get("--threads", "3").parse().unwrap();
    let seed0: u64 = get("--seed", "1").parse().unwrap();
    let pa = parse_params(&get("--a", ""));
    let pb_s = get("--b", "");
    let pb = if pb_s.is_empty() { None } else { Some(parse_params(&pb_s)) };
    let mix = get("--mix", "bot:5,starter:3,brawler:3,house:4");
    let verbose = args.iter().any(|a| a == "-v");
    let hello = json::parse(&std::fs::read_to_string(&hello_path).expect("hello")).expect("hello json");
    let cfg = Config::from(hello.get("config"));
    let world = Arc::new(World::load(&map).expect("map"));
    let nav = Arc::new(Nav::build(&world, 10.0));
    let variants: Vec<(String, Params)> = match pb {
        Some(pb) => vec![("A".into(), pa), ("B".into(), pb)],
        None => vec![("A".into(), pa)],
    };
    for (name, p) in variants {
        let results = Arc::new(Mutex::new(Vec::new()));
        let others_acc = Arc::new(Mutex::new(Vec::new()));
        let next = Arc::new(Mutex::new(0usize));
        let t0 = std::time::Instant::now();
        std::thread::scope(|sc| {
            for _ in 0..threads {
                let results = results.clone();
                let others_acc = others_acc.clone();
                let next = next.clone();
                let (cfg, world, nav, p, mode, mix) = (&cfg, &world, &nav, &p, &mode, &mix);
                sc.spawn(move || loop {
                    let k = {
                        let mut g = next.lock().unwrap();
                        let k = *g;
                        *g += 1;
                        k
                    };
                    if k >= n {
                        break;
                    }
                    let (r, o) = run_match(cfg, world, nav, mode, mix, seed0 + k as u64, p, verbose);
                    results.lock().unwrap().push((k, r));
                    others_acc.lock().unwrap().extend(o);
                });
            }
        });
        let rs = results.lock().unwrap();
        let score: f64 = rs.iter().map(|(_, r)| 16.0 - r.place as f64).sum::<f64>() / rs.len() as f64;
        let sd: f64 = {
            let m = score;
            (rs.iter().map(|(_, r)| (16.0 - r.place as f64 - m).powi(2)).sum::<f64>() / (rs.len() as f64 - 1.0).max(1.0)).sqrt() / (rs.len() as f64).sqrt()
        };
        let kills: f64 = rs.iter().map(|(_, r)| r.kills as f64).sum::<f64>() / rs.len() as f64;
        let dmg: f64 = rs.iter().map(|(_, r)| r.dmg).sum::<f64>() / rs.len() as f64;
        let surv: f64 = rs.iter().map(|(_, r)| r.survive_t).sum::<f64>() / rs.len() as f64;
        let wins = rs.iter().filter(|(_, r)| r.place == 1).count();
        println!(
            "{} [{}] n={} score {:.2} ± {:.2}  kills {:.2}  dmg {:.0}  surv {:.0}s  wins {}  ({:.1}s)",
            name,
            mode,
            rs.len(),
            score,
            sd,
            kills,
            dmg,
            surv,
            wins,
            t0.elapsed().as_secs_f64()
        );
        if mode != "practice" {
            let oa = others_acc.lock().unwrap();
            let mut names: Vec<String> = oa.iter().map(|x| x.0.clone()).collect();
            names.sort();
            names.dedup();
            for nm in names {
                let v: Vec<f64> = oa.iter().filter(|x| x.0 == nm).map(|x| 16.0 - x.1 as f64).collect();
                println!("   {:>14}: {:.2} (n={})", nm, v.iter().sum::<f64>() / v.len() as f64, v.len());
            }
        }
    }
}
