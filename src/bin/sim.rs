//! Local match runner. Usage:
//!   sim --hello HELLO.json --map DIR [--mode practice|ffa] [--n 48] [--threads 3] [--seed 1]
//!       [--a "k=v,..."] [--b "k=v,..."] [--mix "bot:4,starter:4,brawler:4,house:4"]
//! Paired runs: the same seeds are played with params A and with params B.

use mechbot::agents::*;
use mechbot::bot::Params;
use mechbot::json;
use mechbot::nav::Nav;
use mechbot::proto::{Config, WK};
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
            "drop_range" => p.drop_range = f,
            "silent" => p.silent = f != 0.0,
            "reload_cover" => p.reload_cover = f != 0.0,
            "farm_hp" => p.farm_hp = f,
            "new_pref" => p.new_pref = f != 0.0,
            "passive" => p.passive = f != 0.0,
            "npc_fix" => p.npc_fix = f != 0.0,
            "upg_v2" => p.upg_v2 = f != 0.0,
            "stand_no_cover" => p.stand_no_cover = f != 0.0,
            "face_unseen" => p.face_unseen = f != 0.0,
            "w_mortar" => p.wpref[1] = f,
            "w_laser" => p.wpref[2] = f,
            "w_lightning" => p.wpref[3] = f,
            _ => eprintln!("unknown param {}", k),
        }
    }
    p
}

struct Res {
    acc: (u32, u32, u32, u32),
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
    let mut rank_init = vec![0i32; n];
    let mut agents: Vec<Box<dyn Agent>> = Vec::new();
    let intel = Rc::new(RefCell::new(Intel::default()));
    let mut rank = rank_init.clone();
    rank_init.clear();
    let mut cfg_local = cfg.clone();
    let duel_skill = mix.split(',').next().unwrap_or("expert").to_string();
    if mode == "duel" {
        cfg_local.mechs = 2;
        cfg_local.npc_count = 0;
        cfg_local.crate_count = 0;
        cfg_local.floor_count = 0;
    }
    let cfg = &cfg_local;
    let n = cfg.mechs;
    let me = if mode == "duel" { 0 } else { me };
    if mode == "duel" {
        let sk = match duel_skill.as_str() {
            "easy" => Skill::Easy,
            "medium" => Skill::Medium,
            "hard" => Skill::Hard,
            _ => Skill::Expert,
        };
        squad = vec![false, true];
        rank = vec![0, 0];
        agents.push(mk_bot(cfg, world, nav, seed ^ 0xabc, 0, params.clone(), "me"));
        agents.push(Box::new(House::new(1, sk, (**nav).clone(), cfg.clone(), world.clone(), seed.wrapping_add(77), intel.clone(), vec![1])));
    } else if mode == "practice" {
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
                if std::env::var("ME").map_or(false, |v| v == "starter") {
                    agents.push(Box::new(Starter { reach: cfg.pickup_radius }));
                } else {
                    agents.push(mk_bot(cfg, world, nav, seed ^ 0xabc, i as i32, params.clone(), "me"));
                }
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
    let mc = MatchCfg { seed, squad: squad.clone(), max_t: if mode == "duel" { 90.0 } else { cfg.duration_s } };
    let mut sim = Sim::new(cfg.clone(), world.clone(), nav.clone(), &mc);
    if mode == "practice" || mode == "duel" {
        sim.intel = Some(intel.clone());
        sim.rank = rank;
        if mode == "duel" {
            sim.all_hunt_t = 0.0;
        }
        sim.prey_known = mode == "duel";
    }
    if mode == "duel" {
        let kit = |sim: &mut Sim, i: usize, spec: &str| {
            let ks: Vec<WK> = spec.split('+').filter_map(WK::from_str).collect();
            if ks.is_empty() {
                return;
            }
            for s in 0..2 {
                sim.mechs[i].weapons[s] = ks.get(s).map(|k| mechbot::sim::WState { kind: *k, cooldown: 0.0, ammo: if *k == WK::Gun { 12.0 } else { 0.0 }, reload: 0.0, charge: 0.0, hold: 0.0, equip: 0.0 });
            }
        };
        let mk = std::env::var("KIT").unwrap_or_default();
        let hk = std::env::var("HKIT").unwrap_or_default();
        kit(&mut sim, 0, &mk);
        kit(&mut sim, 1, &hk);
        // put the hunter 90-150 m away on open ground
        let p0 = sim.mechs[0].pos;
        for _ in 0..200 {
            let a = rng.range(-3.14159, 3.14159);
            let r = rng.range(90.0, 150.0);
            let (x, y) = (p0[0] + r * a.cos(), p0[1] + r * a.sin());
            if let Some(k) = nav.idx(x, y) {
                if nav.stand[k] && nav.ground[k] {
                    sim.mechs[1].pos = [x, y, world.ground(x, y)];
                    break;
                }
            }
        }
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
    (Res { acc: (m.gun_shots, m.gun_hits, sim.mechs.iter().enumerate().filter(|(i, _)| *i != me).map(|(_, x)| x.gun_shots).sum(), sim.mechs.iter().enumerate().filter(|(i, _)| *i != me).map(|(_, x)| x.gun_hits).sum()), place: place[me], kills: m.kills, dmg: m.dmg_dealt, survive_t: surv }, others)
}

/// 16-mech free-for-all among our own bot: half the slots play params A, half params B.
/// Returns mean (16 - place) for A and B in this match.
fn run_selfplay(cfg: &Config, world: &Arc<World>, nav: &Arc<Nav>, seed: u64, pa: &Params, pb: &Params) -> (f64, f64) {
    let n = cfg.mechs;
    let mut rng = mechbot::geom::Rng(seed.wrapping_mul(2862933555777941757).wrapping_add(3037000493) | 1);
    let mut grp: Vec<usize> = (0..n).map(|i| i % 2).collect();
    for i in (1..n).rev() {
        let j = (rng.next() % (i as u64 + 1)) as usize;
        grp.swap(i, j);
    }
    let mut agents: Vec<Box<dyn Agent>> = Vec::new();
    for i in 0..n {
        let p = if grp[i] == 0 { pa.clone() } else { pb.clone() };
        agents.push(mk_bot(cfg, world, nav, seed.wrapping_add(i as u64 * 7919), i as i32, p, if grp[i] == 0 { "A" } else { "B" }));
    }
    let mc = MatchCfg { seed, squad: vec![false; n], max_t: cfg.duration_s };
    let mut sim = Sim::new(cfg.clone(), world.clone(), nav.clone(), &mc);
    let place = sim.run(&mut agents);
    let mut s = [0.0, 0.0];
    let mut c = [0.0, 0.0];
    for i in 0..n {
        s[grp[i]] += 16.0 - place[i] as f64;
        c[grp[i]] += 1.0;
    }
    (s[0] / c[0], s[1] / c[1])
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
    let pb: Option<Params> = if pb_s.is_empty() { None } else { Some(parse_params(&pb_s)) };
    let mix = get("--mix", "bot:5,starter:3,brawler:3,house:4");
    let verbose = args.iter().any(|a| a == "-v");
    let hello = json::parse(&std::fs::read_to_string(&hello_path).expect("hello")).expect("hello json");
    let cfg = Config::from(hello.get("config"));
    let world = Arc::new(World::load(&map).expect("map"));
    let nav = Arc::new(Nav::build(&world, 10.0));
    if mode == "selfplay" {
        let pb = pb.clone().unwrap_or_default();
        let res = Arc::new(Mutex::new(Vec::new()));
        let next = Arc::new(Mutex::new(0usize));
        let t0 = std::time::Instant::now();
        std::thread::scope(|sc| {
            for _ in 0..threads {
                let (res, next) = (res.clone(), next.clone());
                let (cfg, world, nav, pa, pb) = (&cfg, &world, &nav, &pa, &pb);
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
                    let r = run_selfplay(cfg, world, nav, seed0 + k as u64, pa, pb);
                    if verbose {
                        eprintln!("seed {} A {:.2} B {:.2}", seed0 + k as u64, r.0, r.1);
                    }
                    res.lock().unwrap().push(r);
                });
            }
        });
        let rs = res.lock().unwrap();
        let m = rs.len() as f64;
        let d: Vec<f64> = rs.iter().map(|r| r.0 - r.1).collect();
        let md = d.iter().sum::<f64>() / m;
        let sd = (d.iter().map(|x| (x - md).powi(2)).sum::<f64>() / (m - 1.0).max(1.0)).sqrt() / m.sqrt();
        println!(
            "[selfplay] n={} A {:.2}  B {:.2}  A-B {:+.2} ± {:.2}  ({:.1}s)",
            rs.len(),
            rs.iter().map(|r| r.0).sum::<f64>() / m,
            rs.iter().map(|r| r.1).sum::<f64>() / m,
            md,
            sd,
            t0.elapsed().as_secs_f64()
        );
        return;
    }
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
        let (ms, mh, os, oh) = rs.iter().fold((0u32, 0u32, 0u32, 0u32), |a, (_, r)| (a.0 + r.acc.0, a.1 + r.acc.1, a.2 + r.acc.2, a.3 + r.acc.3));
        println!("   gun accuracy: me {:.2} ({} shots)  others {:.2} ({} shots)", mh as f64 / ms.max(1) as f64, ms, oh as f64 / os.max(1) as f64, os);
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
