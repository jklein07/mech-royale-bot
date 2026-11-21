//! Local approximation of the mechfight/1 server for offline A/B testing.
//! Written from protocol.md; it is NOT the real server. Treat results as directional.

use crate::geom::*;
use crate::nav::Nav;
use crate::proto::*;
use crate::world::World;
use std::f64::consts::PI;
use std::sync::Arc;

use std::cell::RefCell;
use std::rc::Rc;

/// Squad-shared intel (house squad in practice mode).
#[derive(Default, Clone)]
pub struct Intel {
    pub t: f64,
    pub prey: Option<(i32, V3, V3)>,
    pub hunters: Vec<i32>,
}

pub trait Agent {
    fn act(&mut self, o: &Obs) -> Action;
    fn name(&self) -> String;
}

#[derive(Clone)]
pub struct WState {
    pub kind: WK,
    pub cooldown: f64,
    pub ammo: f64,
    pub reload: f64,
    pub charge: f64,
    pub hold: f64, // time held at full charge
    pub equip: f64,
}

#[derive(Clone)]
pub struct MechS {
    pub id: i32,
    pub alive: bool,
    pub pos: V3,
    pub vel: V3,
    pub aim: [f64; 2],
    pub cmd: Action,
    pub hp: f64,
    pub energy: f64,
    pub cubes: f64,
    pub upgrades: Vec<String>,
    pub ups_count: Vec<(String, u32)>,
    pub weapons: [Option<WState>; 2],
    pub base: MStats,
    pub stats: MStats,
    pub wstats: [WStats; 4],
    pub grounded: bool,
    pub last_spend: f64,
    pub last_combat: f64,
    pub zone_out_t: f64,
    pub death_tick: i64,
    pub kills: i32,
    pub events: Vec<Event>,
    pub seen: Vec<f64>, // last time this mech saw mech j (for memory)
    pub seen_npc: Vec<f64>,
    pub offers: Vec<(i64, Vec<String>, f64)>,
    pub next_threshold: usize,
    pub missed: f64,
    pub short_until: f64,
    pub step_acc: f64,
    pub dmg_dealt: f64,
    pub gun_shots: u32,
    pub gun_hits: u32,
    pub last_fire: [bool; 2],
}

#[derive(Clone)]
pub struct Npc {
    pub id: i32,
    pub home: V3,
    pub pos: V3,
    pub vel: V3,
    pub aim: f64,
    pub hp: f64,
    pub alive: bool,
    pub target: Option<i32>,
    pub cooldown: f64,
    pub wander: (f64, f64),
    pub wander_t: f64,
}

#[derive(Clone)]
pub struct Proj {
    pub id: i64,
    pub kind: u8, // 0 gun 1 mortar 2 npc
    pub owner: i32,
    pub pos: V3,
    pub vel: V3,
    pub dmg: f64,
    pub travelled: f64,
    pub range: f64,
    pub ttl: f64,
    pub splash: f64,
    pub splash_r: f64,
    pub edge: f64,
    pub knock: f64,
    pub knock_up: f64,
    pub drain: f64,
}

#[derive(Clone)]
pub struct Crate {
    pub id: i64,
    pub pos: V3,
    pub hp: f64,
}

#[derive(Clone)]
pub struct PickupS {
    pub id: i64,
    pub kind: WK,
    pub pos: V3,
    pub ammo: f64,
}

#[derive(Clone)]
pub struct DropS {
    pub id: i64,
    pub pos: V3,
    pub call_t: f64,
    pub land_t: f64,
    pub holder: Option<i32>,
    pub progress: f64,
    pub taken: bool,
}

#[derive(Clone)]
pub struct SoundS {
    pub kind: &'static str,
    pub pos: V3,
    pub src: i32,
    pub t: f64,
}

pub struct MatchCfg {
    pub seed: u64,
    pub squad: Vec<bool>, // per slot: member of the house squad
    pub max_t: f64,
}

pub struct Sim {
    pub cfg: Config,
    pub world: Arc<World>,
    pub nav: Arc<Nav>,
    pub rng: Rng,
    pub tick: i64,
    pub t: f64,
    pub mechs: Vec<MechS>,
    pub npcs: Vec<Npc>,
    pub projs: Vec<Proj>,
    pub crates: Vec<Crate>,
    pub cubes: Vec<Item>,
    pub pickups: Vec<PickupS>,
    pub drops: Vec<DropS>,
    pub sounds: Vec<SoundS>,
    pub beams: Vec<(i32, bool, Vec<V3>, f64)>,
    pub next_id: i64,
    pub zone_c: [f64; 2],
    pub zone_r: f64,
    pub phase: usize,
    pub phase_start: f64,
    pub next_c: [f64; 2],
    pub next_r: f64,
    pub squad: Vec<bool>,
    pub done: bool,
    pub place: Vec<i32>,
    pub max_t: f64,
    /// squad shared knowledge: last seen time of each mech by any squad member
    pub squad_seen: Vec<f64>,
    pub intel: Option<Rc<RefCell<Intel>>>,
    /// weakness rank per slot (lower hunts first)
    pub rank: Vec<i32>,
    pub all_hunt_t: f64,
    /// hunters always know the prey position (duel tests)
    pub prey_known: bool,
}

fn sstat(st: &MStats, path: &str) -> Option<f64> {
    let mut s = st.clone();
    s.get_mut(path).map(|v| *v)
}

impl Sim {
    pub fn new(cfg: Config, world: Arc<World>, nav: Arc<Nav>, mc: &MatchCfg) -> Sim {
        let n = cfg.mechs;
        let mut sim = Sim {
            rng: Rng(mc.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) | 1),
            tick: 0,
            t: 0.0,
            mechs: Vec::new(),
            npcs: Vec::new(),
            projs: Vec::new(),
            crates: Vec::new(),
            cubes: Vec::new(),
            pickups: Vec::new(),
            drops: Vec::new(),
            sounds: Vec::new(),
            beams: Vec::new(),
            next_id: 0,
            zone_c: cfg.zone_start_center,
            zone_r: cfg.zone_start_radius,
            phase: 0,
            phase_start: 0.0,
            next_c: cfg.zone_start_center,
            next_r: cfg.zone_start_radius,
            squad: mc.squad.clone(),
            done: false,
            place: vec![0; n],
            max_t: mc.max_t,
            squad_seen: vec![-100.0; n],
            intel: None,
            rank: vec![0; n],
            all_hunt_t: 240.0,
            prey_known: false,
            cfg,
            world,
            nav,
        };
        for _ in 0..20 {
            sim.rng.next();
        }
        sim.pick_next_zone();
        // spawns
        let mut spawns: Vec<V3> = Vec::new();
        let mut tries = 0;
        while spawns.len() < n && tries < 20000 {
            tries += 1;
            let sep = if tries > 10000 { sim.cfg.spawn_min_sep * 0.6 } else { sim.cfg.spawn_min_sep };
            if let Some(p) = sim.random_ground(sim.cfg.zone_start_center, sim.cfg.zone_start_radius * sim.cfg.spawn_zone_fraction, sim.cfg.arena_margin) {
                if spawns.iter().all(|s| d2(s, &p) >= sep) {
                    spawns.push(p);
                }
            }
        }
        while spawns.len() < n {
            let p = sim.random_ground(sim.cfg.zone_start_center, 400.0, 30.0).unwrap_or([0.0, 0.0, 0.0]);
            spawns.push(p);
        }
        // shuffle
        for i in (1..n).rev() {
            let j = (sim.rng.next() % (i as u64 + 1)) as usize;
            spawns.swap(i, j);
        }
        let base = sim.cfg.mech.clone();
        for i in 0..n {
            let mut w0 = WState { kind: WK::Gun, cooldown: 0.0, ammo: sim.cfg.w(WK::Gun).mag, reload: 0.0, charge: 0.0, hold: 0.0, equip: 0.0 };
            w0.ammo = sim.cfg.w(WK::Gun).mag;
            let aim0 = sim.rng.range(-PI, PI);
            sim.mechs.push(MechS {
                id: i as i32,
                alive: true,
                pos: spawns[i],
                vel: [0.0; 3],
                aim: [aim0, 0.0],
                cmd: Action { aim: Some([aim0, 0.0]), ..Default::default() },
                hp: base.hp,
                energy: base.energy,
                cubes: 0.0,
                upgrades: vec![],
                ups_count: vec![],
                weapons: [Some(w0), None],
                base: base.clone(),
                stats: base.clone(),
                wstats: sim.cfg.weapons.clone(),
                grounded: true,
                last_spend: -100.0,
                last_combat: -100.0,
                zone_out_t: -100.0,
                death_tick: -1,
                kills: 0,
                events: vec![],
                seen: vec![-100.0; n],
                seen_npc: vec![-100.0; 64],
                offers: vec![],
                next_threshold: 0,
                missed: 0.0,
                short_until: -1.0,
                step_acc: 0.0,
                dmg_dealt: 0.0,
                gun_shots: 0,
                gun_hits: 0,
                last_fire: [false, false],
            });
        }
        // npcs
        for k in 0..sim.cfg.npc_count {
            if let Some(p) = sim.random_ground(sim.cfg.zone_start_center, sim.cfg.zone_start_radius * 0.85, 40.0) {
                sim.npcs.push(Npc {
                    id: 100 + k as i32,
                    home: p,
                    pos: p,
                    vel: [0.0; 3],
                    aim: 0.0,
                    hp: sim.cfg.npc_hp,
                    alive: true,
                    target: None,
                    cooldown: 0.0,
                    wander: (p[0], p[1]),
                    wander_t: 0.0,
                });
            }
        }
        // crates
        for k in 0..sim.cfg.crate_count {
            let roof = (k as f64) < sim.cfg.crate_count as f64 * sim.cfg.crate_roof_fraction;
            let p = if roof { sim.random_roof() } else { sim.random_ground(sim.cfg.zone_start_center, sim.cfg.zone_start_radius * 0.9, 25.0) };
            if let Some(p) = p {
                let id = 1000 + k as i64;
                sim.crates.push(Crate { id, pos: p, hp: sim.cfg.crate_hp });
            }
        }
        // floor loot
        sim.next_id = 8000;
        for k in 0..sim.cfg.floor_count {
            let roof = (k as f64) < sim.cfg.floor_count as f64 * sim.cfg.floor_roof_fraction;
            let p = if roof { sim.random_roof() } else { sim.random_ground(sim.cfg.zone_start_center, sim.cfg.zone_start_radius * 0.9, 25.0) };
            if let Some(p) = p {
                let kind = sim.random_weapon();
                let id = sim.next_id;
                sim.next_id += 1;
                sim.pickups.push(PickupS { id, kind, pos: p, ammo: 0.0 });
            }
        }
        sim
    }

    fn random_weapon(&mut self) -> WK {
        let w = self.cfg.loot_weights;
        let tot: f64 = w.iter().sum();
        let mut r = self.rng.f() * tot;
        for k in WK::ALL {
            r -= w[k as usize];
            if r <= 0.0 {
                return k;
            }
        }
        WK::Laser
    }

    pub fn random_ground(&mut self, c: [f64; 2], r: f64, margin: f64) -> Option<V3> {
        for _ in 0..200 {
            let a = self.rng.range(-PI, PI);
            let rr = r * self.rng.f().sqrt();
            let x = c[0] + rr * a.cos();
            let y = c[1] + rr * a.sin();
            let b = self.world.bounds;
            if x < b[0] + margin || x > b[1] - margin || y < b[2] + margin || y > b[3] - margin {
                continue;
            }
            if let Some(k) = self.nav.idx(x, y) {
                if self.nav.stand[k] && self.nav.ground[k] {
                    return Some([x, y, self.world.ground(x, y)]);
                }
            }
        }
        None
    }

    fn random_roof(&mut self) -> Option<V3> {
        for _ in 0..400 {
            let b = self.world.bounds;
            let x = self.rng.range(b[0] + 30.0, b[1] - 30.0);
            let y = self.rng.range(b[2] + 30.0, b[3] - 30.0);
            if d2(&[x, y], &self.cfg.zone_start_center) > self.cfg.zone_start_radius * 0.9 {
                continue;
            }
            if let Some(k) = self.world.cell(x, y) {
                let r = self.world.roof[k] as f64;
                let g = self.world.ground(x, y);
                if r > g + 4.0 && r - g < 60.0 {
                    // flat-ish roof: all neighbors same height
                    let ok = [(2.0, 0.0), (-2.0, 0.0), (0.0, 2.0), (0.0, -2.0)].iter().all(|(dx, dy)| (self.world.roof_at(x + dx, y + dy) - r).abs() < 0.5);
                    if ok {
                        return Some([x, y, r]);
                    }
                }
            }
        }
        None
    }

    fn pick_next_zone(&mut self) {
        if self.phase >= self.cfg.zone_phases.len() {
            self.next_c = self.zone_c;
            self.next_r = 0.0;
            return;
        }
        let nr = self.cfg.zone_phases[self.phase].radius;
        let slack = (self.zone_r - nr).max(0.0);
        for _ in 0..100 {
            let a = self.rng.range(-PI, PI);
            let rr = slack * self.rng.f().sqrt();
            let c = [self.zone_c[0] + rr * a.cos(), self.zone_c[1] + rr * a.sin()];
            let b = self.world.bounds;
            // keep a good chunk inside the map
            if c[0] < b[0] + 40.0 || c[0] > b[1] - 40.0 || c[1] < b[2] + 40.0 || c[1] > b[3] - 40.0 {
                continue;
            }
            self.next_c = c;
            self.next_r = nr;
            return;
        }
        self.next_c = self.zone_c;
        self.next_r = nr;
    }

    pub fn zone_now(&self) -> ([f64; 2], f64, f64, f64, f64) {
        // returns center, radius, shrink_start, shrink_end, dps
        if self.phase >= self.cfg.zone_phases.len() {
            return (self.zone_c, self.zone_r, self.phase_start, self.phase_start, self.cfg.zone_phases.last().map_or(260.0, |p| p.dps));
        }
        let ph = &self.cfg.zone_phases[self.phase];
        let ss = self.phase_start + ph.hold_s;
        let se = ss + ph.shrink_s;
        let f = if self.t <= ss { 0.0 } else { ((self.t - ss) / ph.shrink_s.max(1e-6)).min(1.0) };
        let c = [self.zone_c[0] + (self.next_c[0] - self.zone_c[0]) * f, self.zone_c[1] + (self.next_c[1] - self.zone_c[1]) * f];
        let r = self.zone_r + (self.next_r - self.zone_r) * f;
        (c, r, ss, se, ph.dps)
    }

    fn zone_obs(&self) -> Zone {
        let (_, _, ss, se, dps) = self.zone_now();
        Zone { center: self.zone_c, radius: self.zone_r, next_center: self.next_c, next_radius: self.next_r, shrink_start: ss, shrink_end: se, dps }
    }

    fn scale(&self, m: &MechS) -> f64 {
        1.0 + self.cfg.size_per_cube * m.cubes
    }

    fn recompute_stats(&mut self, i: usize) {
        let cubes = self.mechs[i].cubes;
        let mut st = self.mechs[i].base.clone();
        let mut ws = self.cfg.weapons.clone();
        for (uid, cnt) in self.mechs[i].ups_count.clone() {
            if let Some(u) = self.cfg.upgrades.iter().find(|u| u.id == uid) {
                for _ in 0..cnt {
                    for e in &u.effects {
                        let parts: Vec<&str> = e.path.split('.').collect();
                        let target: Option<&mut f64> = if parts[0] == "mech" {
                            st.get_mut(parts[1])
                        } else if parts[0] == "weapons" && parts.len() == 3 {
                            WK::from_str(parts[1]).and_then(|k| ws[k as usize].get_mut(parts[2]))
                        } else {
                            None
                        };
                        if let Some(v) = target {
                            if let Some(m) = e.mul {
                                *v *= m;
                            }
                            if let Some(a) = e.add {
                                *v += a;
                            }
                        }
                    }
                }
            }
        }
        let sc = 1.0 + self.cfg.size_per_cube * cubes;
        st.radius = self.cfg.mech.radius * sc;
        st.height = self.cfg.mech.height * sc;
        st.eye_height = self.cfg.mech.eye_height * sc;
        st.muzzle_height = self.cfg.mech.muzzle_height * sc;
        let hpm = 1.0 + self.cfg.hp_per_cube * cubes.min(self.cfg.stat_cap);
        let new_max = st.hp * hpm;
        let old_max = self.mechs[i].stats.hp * (1.0 + self.cfg.hp_per_cube * self.mechs[i].cubes_prev().min(self.cfg.stat_cap));
        let m = &mut self.mechs[i];
        if new_max > old_max {
            m.hp += new_max - old_max;
        }
        st.hp = new_max;
        m.hp = m.hp.min(new_max);
        m.stats = st;
        m.wstats = ws;
    }

    pub fn max_hp(&self, i: usize) -> f64 {
        self.mechs[i].stats.hp
    }

    fn give_cubes(&mut self, i: usize, n: f64) {
        for _ in 0..(n as i32) {
            self.mechs[i].cubes += 1.0;
            let c = self.mechs[i].cubes;
            // max hp +4% heals the difference: handled in recompute via cubes_prev
            self.recompute_stats_cube(i);
            let th = self.cfg.upgrade_thresholds.clone();
            while self.mechs[i].next_threshold < th.len() && c >= th[self.mechs[i].next_threshold] {
                self.mechs[i].next_threshold += 1;
                let choices = self.draw_offer(i);
                if !choices.is_empty() {
                    let id = self.next_id;
                    self.next_id += 1;
                    let exp = self.t + 5.0;
                    self.mechs[i].offers.push((id, choices, exp));
                }
            }
        }
    }

    fn recompute_stats_cube(&mut self, i: usize) {
        // apply +hp% for the new cube (heals the difference) then recompute everything
        let cap = self.cfg.stat_cap;
        let c = self.mechs[i].cubes;
        let before = self.mechs[i].stats.hp;
        self.mechs[i].prev_cubes_set(c);
        self.recompute_stats_raw(i);
        let after = self.mechs[i].stats.hp;
        if c <= cap && after > before {
            self.mechs[i].hp += after - before;
        }
    }

    fn recompute_stats_raw(&mut self, i: usize) {
        let cubes = self.mechs[i].cubes;
        let mut st = self.mechs[i].base.clone();
        let mut ws = self.cfg.weapons.clone();
        for (uid, cnt) in self.mechs[i].ups_count.clone() {
            if let Some(u) = self.cfg.upgrades.iter().find(|u| u.id == uid) {
                for _ in 0..cnt {
                    for e in &u.effects {
                        let parts: Vec<&str> = e.path.split('.').collect();
                        let target: Option<&mut f64> = if parts[0] == "mech" && parts.len() == 2 {
                            st.get_mut(parts[1])
                        } else if parts[0] == "weapons" && parts.len() == 3 {
                            WK::from_str(parts[1]).and_then(|k| ws[k as usize].get_mut(parts[2]))
                        } else {
                            None
                        };
                        if let Some(v) = target {
                            if let Some(m) = e.mul {
                                *v *= m;
                            }
                            if let Some(a) = e.add {
                                *v += a;
                            }
                        }
                    }
                }
            }
        }
        let sc = 1.0 + self.cfg.size_per_cube * cubes;
        st.radius = self.cfg.mech.radius * sc;
        st.height = self.cfg.mech.height * sc;
        st.eye_height = self.cfg.mech.eye_height * sc;
        st.muzzle_height = self.cfg.mech.muzzle_height * sc;
        st.hp *= 1.0 + self.cfg.hp_per_cube * cubes.min(self.cfg.stat_cap);
        let m = &mut self.mechs[i];
        m.hp = m.hp.min(st.hp);
        m.energy = m.energy.min(st.energy);
        m.stats = st;
        m.wstats = ws;
    }

    fn draw_offer(&mut self, i: usize) -> Vec<String> {
        let held: Vec<WK> = self.mechs[i].weapons.iter().flatten().map(|w| w.kind).collect();
        let mut elig: Vec<String> = Vec::new();
        for u in &self.cfg.upgrades {
            if let Some(r) = u.requires {
                if !held.contains(&r) {
                    continue;
                }
            }
            let have = self.mechs[i].ups_count.iter().find(|x| x.0 == u.id).map_or(0, |x| x.1);
            if have >= u.max {
                continue;
            }
            elig.push(u.id.clone());
        }
        let mut out = Vec::new();
        while out.len() < 3 && !elig.is_empty() {
            let k = (self.rng.next() % elig.len() as u64) as usize;
            out.push(elig.remove(k));
        }
        out
    }

    fn apply_upgrade(&mut self, i: usize, uid: &str) {
        let m = &mut self.mechs[i];
        if let Some(x) = m.ups_count.iter_mut().find(|x| x.0 == uid) {
            x.1 += 1;
        } else {
            m.ups_count.push((uid.to_string(), 1));
        }
        m.upgrades.push(uid.to_string());
        m.events.push(Event { typ: "upgrade".into(), kind: uid.to_string(), ..Default::default() });
        let before = self.mechs[i].stats.hp;
        self.recompute_stats_raw(i);
        let after = self.mechs[i].stats.hp;
        if after > before {
            self.mechs[i].hp += after - before;
        }
    }

    fn sound(&mut self, kind: &'static str, pos: V3, src: i32) {
        self.sounds.push(SoundS { kind, pos, src, t: self.t });
    }

    /// Solid-column test for movement.
    fn blocked_at(&self, x: f64, y: f64, z: f64, r: f64, h: f64, step: f64) -> bool {
        let pts = 8;
        let mut check = |px: f64, py: f64| -> bool {
            match self.world.cell(px, py) {
                None => true,
                Some(k) => {
                    let w = &self.world;
                    let ter = w.terrain[k] as f64;
                    if ter > z + step {
                        return true;
                    }
                    let rf = w.roof[k] as f64;
                    if rf > z + step && (w.rbase[k] as f64) < z + h {
                        return true;
                    }
                    let bt = w.btop[k] as f64;
                    if bt > z + step && (w.bbase[k] as f64) < z + h {
                        return true;
                    }
                    false
                }
            }
        };
        if check(x, y) {
            return true;
        }
        for k in 0..pts {
            let a = k as f64 / pts as f64 * 2.0 * PI;
            if check(x + r * a.cos(), y + r * a.sin()) {
                return true;
            }
            if check(x + r * 0.55 * a.cos(), y + r * 0.55 * a.sin()) {
                return true;
            }
        }
        false
    }

    /// Height of the floor under (x,y) for feet at z (highest top <= z + step).
    fn floor_at(&self, x: f64, y: f64, z: f64, step: f64) -> f64 {
        match self.world.cell(x, y) {
            None => z,
            Some(k) => {
                let w = &self.world;
                let mut f = w.ground(x, y);
                let rf = w.roof[k] as f64;
                if rf > f && rf <= z + step {
                    f = rf;
                }
                let bt = w.btop[k] as f64;
                if bt > f && bt <= z + step {
                    f = bt;
                }
                f
            }
        }
    }

    fn move_mech(&mut self, i: usize, dt: f64) {
        let charging = self.mechs[i].weapons.iter().flatten().any(|w| w.kind == WK::Laser && w.charge > 0.0);
        let m = &self.mechs[i];
        let st = m.stats.clone();
        let cmd = m.cmd.clone();
        let (mut mx, mut my) = (cmd.mv[0], cmd.mv[1]);
        let l = (mx * mx + my * my).sqrt();
        if l > 1.0 {
            mx /= l;
            my /= l;
        }
        let moving = l > 0.05;
        let mut energy = m.energy;
        let mut spent = false;
        let mut speed = st.walk_speed;
        let mut boosting = false;
        if cmd.boost && m.grounded && moving && energy > 0.0 {
            speed = st.boost_speed;
            energy -= st.boost_drain * dt;
            spent = true;
            boosting = true;
        }
        if charging {
            speed *= st.charge_move_mult;
        }
        let des = [mx * speed, my * speed];
        let acc = if m.grounded { st.ground_accel } else { st.air_accel };
        let mut vel = m.vel;
        for k in 0..2 {
            let dv = des[k] - vel[k];
            let maxdv = acc * dt;
            vel[k] += dv.clamp(-maxdv, maxdv);
        }
        // vertical
        let mut grounded = m.grounded;
        if cmd.jump && grounded {
            vel[2] = st.jump_speed;
            grounded = false;
            self.sounds.push(SoundS { kind: "jet", pos: m.pos, src: m.id, t: self.t });
        }
        if !grounded && cmd.boost && energy > 0.0 {
            vel[2] = (vel[2] + st.jet_accel * dt).min(st.max_rise_speed.max(vel[2]));
            energy -= st.jet_drain * dt;
            spent = true;
        }
        if !grounded {
            vel[2] -= st.gravity * dt;
        }
        let mut pos = m.pos;
        let r = st.radius;
        let h = st.height;
        // horizontal with slide
        let nx = pos[0] + vel[0] * dt;
        let ny = pos[1] + vel[1] * dt;
        let step = st.step_up;
        let slope_ok = |sim: &Sim, ax: f64, ay: f64, bx: f64, by: f64, z: f64| -> bool {
            if !grounded {
                return true;
            }
            let fa = sim.floor_at(ax, ay, z, step);
            let fb = sim.floor_at(bx, by, z, step);
            let dh = fb - fa;
            let dd = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
            dh <= 0.05 || dh <= dd * 1.19 + 0.02 || dh > 0.6 // >0.6: it's a ledge step, allowed by step_up
        };
        if !self.blocked_at(nx, ny, pos[2], r, h, step) && slope_ok(self, pos[0], pos[1], nx, ny, pos[2]) {
            pos[0] = nx;
            pos[1] = ny;
        } else if !self.blocked_at(nx, pos[1], pos[2], r, h, step) && slope_ok(self, pos[0], pos[1], nx, pos[1], pos[2]) {
            pos[0] = nx;
            vel[1] *= 0.5;
        } else if !self.blocked_at(pos[0], ny, pos[2], r, h, step) && slope_ok(self, pos[0], pos[1], pos[0], ny, pos[2]) {
            pos[1] = ny;
            vel[0] *= 0.5;
        } else {
            vel[0] = 0.0;
            vel[1] = 0.0;
        }
        // vertical
        let nz = pos[2] + vel[2] * dt;
        let fl = self.floor_at(pos[0], pos[1], pos[2].max(nz), step);
        if grounded {
            // follow the floor (step up or down)
            if fl >= pos[2] - 0.8 {
                pos[2] = fl;
                vel[2] = 0.0;
            } else {
                grounded = false;
                pos[2] = nz;
            }
        } else {
            if nz <= fl {
                pos[2] = fl;
                vel[2] = 0.0;
                grounded = true;
            } else {
                // ceiling (bridges/roofs above) - stop rise if head would enter solid
                if vel[2] > 0.0 && self.world.solid(pos[0], pos[1], nz + h) {
                    vel[2] = 0.0;
                } else {
                    pos[2] = nz;
                }
            }
        }
        // energy regen
        let now = self.t;
        let m = &mut self.mechs[i];
        if spent {
            m.last_spend = now;
        } else if now - m.last_spend >= st.energy_regen_delay {
            energy = (energy + st.energy_regen * dt).min(st.energy);
        }
        m.energy = energy.max(0.0);
        m.pos = pos;
        m.vel = vel;
        m.grounded = grounded;
        // footsteps
        let hv = (vel[0] * vel[0] + vel[1] * vel[1]).sqrt();
        if grounded && hv > self.cfg.step_speed {
            m.step_acc += dt;
            if m.step_acc >= self.cfg.step_every {
                m.step_acc = 0.0;
                let p = m.pos;
                let id = m.id;
                self.sounds.push(SoundS { kind: if boosting { "boost" } else { "step" }, pos: p, src: id, t: now });
            }
        }
        // aim slew
        let m = &mut self.mechs[i];
        if let Some(a) = cmd.aim {
            let tr = st.turn_rate * dt;
            let dy = wrap(a[0] - m.aim[0]);
            m.aim[0] = wrap(m.aim[0] + dy.clamp(-tr, tr));
            let p = a[1].clamp(st.min_pitch, st.max_pitch);
            let pr = st.pitch_rate * dt;
            m.aim[1] += (p - m.aim[1]).clamp(-pr, pr);
        }
    }

    /// Ray vs mech cylinders/npcs/crates. Returns (t, kind, index) of first hit along a->b.
    fn ray_hit_actor(&self, a: V3, b: V3, owner: i32, prad: f64, skip: &[i64]) -> Option<(f64, u8, usize)> {
        let d = sub(b, a);
        let mut best: Option<(f64, u8, usize)> = None;
        let mut test = |t0: f64, kind: u8, idx: usize, best: &mut Option<(f64, u8, usize)>| {
            if best.map_or(true, |bb| t0 < bb.0) {
                *best = Some((t0, kind, idx));
            }
        };
        let seg_cyl = |c: V3, r: f64, h: f64| -> Option<f64> {
            // segment vs vertical cylinder, sample-based along the segment's closest approach
            let dx = d[0];
            let dy = d[1];
            let fx = a[0] - c[0];
            let fy = a[1] - c[1];
            let qa = dx * dx + dy * dy;
            let rr = r + prad;
            let (t_in, t_out) = if qa < 1e-12 {
                if fx * fx + fy * fy <= rr * rr {
                    (0.0, 1.0)
                } else {
                    return None;
                }
            } else {
                let qb = 2.0 * (fx * dx + fy * dy);
                let qc = fx * fx + fy * fy - rr * rr;
                let disc = qb * qb - 4.0 * qa * qc;
                if disc < 0.0 {
                    return None;
                }
                let s = disc.sqrt();
                ((-qb - s) / (2.0 * qa), (-qb + s) / (2.0 * qa))
            };
            let t0 = t_in.max(0.0);
            let t1 = t_out.min(1.0);
            if t0 > t1 {
                return None;
            }
            // vertical overlap within [t0,t1]
            let z0 = a[2] + d[2] * t0;
            let z1 = a[2] + d[2] * t1;
            let lo = c[2] - prad;
            let hi = c[2] + h + prad;
            if (z0 < lo && z1 < lo) || (z0 > hi && z1 > hi) {
                return None;
            }
            // first t in [t0,t1] with z in [lo,hi]
            if z0 >= lo && z0 <= hi {
                return Some(t0);
            }
            let zt = if z0 < lo { lo } else { hi };
            let tt = if (z1 - z0).abs() > 1e-9 { t0 + (zt - z0) / (z1 - z0) * (t1 - t0) } else { t0 };
            Some(tt.clamp(t0, t1))
        };
        for (j, m) in self.mechs.iter().enumerate() {
            if !m.alive || m.id == owner || skip.contains(&(m.id as i64)) {
                continue;
            }
            if let Some(t0) = seg_cyl(m.pos, m.stats.radius, m.stats.height) {
                test(t0, 0, j, &mut best);
            }
        }
        for (j, n) in self.npcs.iter().enumerate() {
            if !n.alive || skip.contains(&(n.id as i64)) {
                continue;
            }
            if let Some(t0) = seg_cyl(n.pos, self.cfg.npc_radius, self.cfg.npc_height) {
                test(t0, 1, j, &mut best);
            }
        }
        for (j, c) in self.crates.iter().enumerate() {
            if c.hp <= 0.0 || skip.contains(&c.id) {
                continue;
            }
            if let Some(t0) = seg_cyl(c.pos, self.cfg.crate_radius, self.cfg.crate_height) {
                test(t0, 2, j, &mut best);
            }
        }
        best
    }

    /// Apply weapon damage from mech `src` (or -1 for npc) to target.
    fn damage(&mut self, src: i32, kind: u8, idx: usize, base: f64, weapon: &str, from: V3, effect: Option<&str>) {
        let now = self.t;
        let mult = if src >= 0 {
            let s = &self.mechs[src as usize];
            1.0 + self.cfg.damage_per_cube * s.cubes.min(self.cfg.stat_cap)
        } else {
            1.0
        };
        let mut dmg = base * mult;
        match kind {
            0 => {
                let tgt = &self.mechs[idx];
                if !tgt.alive {
                    return;
                }
                let yaw_from = yaw_to(&tgt.pos, &from);
                let rear = wrap(yaw_from - tgt.aim[0]).abs() > (tgt.stats.front_arc_deg.to_radians() / 2.0);
                if rear {
                    dmg *= tgt.stats.rear_mult;
                }
                if effect == Some("cookoff") && tgt.weapons.iter().flatten().any(|w| w.kind == WK::Mortar) {
                    dmg *= 1.0 + self.cfg.w(WK::Laser).cookoff;
                }
                dmg *= (1.0 - tgt.stats.armor).max(0.1);
                let tid = tgt.id;
                {
                    let t = &mut self.mechs[idx];
                    t.hp -= dmg;
                    t.last_combat = now;
                    t.events.push(Event {
                        typ: "damaged".into(),
                        amount: (dmg * 100.0).round() / 100.0,
                        weapon: weapon.to_string(),
                        bearing: Some(yaw_from),
                        by: if src >= 0 { Some(src) } else { None },
                        rear,
                        ..Default::default()
                    });
                }
                if src >= 0 {
                    let s = &mut self.mechs[src as usize];
                    s.last_combat = now;
                    s.dmg_dealt += dmg;
                    s.events.push(Event { typ: "hit".into(), target: tid as i64, amount: dmg, weapon: weapon.to_string(), rear, ..Default::default() });
                }
                if self.mechs[idx].hp <= 0.0 {
                    self.kill_mech(idx, src);
                }
            }
            1 => {
                let n = &mut self.npcs[idx];
                if !n.alive {
                    return;
                }
                n.hp -= dmg;
                if src >= 0 {
                    n.target = Some(src);
                    let nid = n.id;
                    let s = &mut self.mechs[src as usize];
                    s.last_combat = now;
                    s.events.push(Event { typ: "hit".into(), target: nid as i64, amount: dmg, weapon: weapon.to_string(), ..Default::default() });
                }
                if self.npcs[idx].hp <= 0.0 {
                    self.npcs[idx].alive = false;
                    let p = self.npcs[idx].pos;
                    for _ in 0..(self.cfg.npc_cubes as i32) {
                        self.spawn_cube(p);
                    }
                    self.sound("death", p, -1);
                }
            }
            _ => {
                let c = &mut self.crates[idx];
                if c.hp <= 0.0 {
                    return;
                }
                c.hp -= dmg;
                let cid = c.id;
                if src >= 0 {
                    self.mechs[src as usize].events.push(Event { typ: "hit".into(), target: cid, amount: dmg, weapon: weapon.to_string(), ..Default::default() });
                }
                if self.crates[idx].hp <= 0.0 {
                    let p = self.crates[idx].pos;
                    for _ in 0..(self.cfg.crate_cubes as i32) {
                        self.spawn_cube(p);
                    }
                    if self.rng.f() < self.cfg.crate_weapon_chance {
                        let k = self.random_weapon();
                        let id = self.next_id;
                        self.next_id += 1;
                        let pp = [p[0] + self.rng.range(-1.5, 1.5), p[1] + self.rng.range(-1.5, 1.5), p[2]];
                        self.pickups.push(PickupS { id, kind: k, pos: pp, ammo: 0.0 });
                    }
                    self.sound("crate", p, src);
                }
            }
        }
    }

    fn spawn_cube(&mut self, p: V3) {
        let id = 5000 + self.next_id;
        self.next_id += 1;
        let a = self.rng.range(-PI, PI);
        let r = self.rng.range(0.5, 3.0);
        let x = p[0] + r * a.cos();
        let y = p[1] + r * a.sin();
        let z = self.floor_at(x, y, p[2] + 0.5, 1.2);
        self.cubes.push(Item { id, pos: [x, y, z], hp: 0.0 });
    }

    fn kill_mech(&mut self, idx: usize, src: i32) {
        let alive_before = self.mechs.iter().filter(|m| m.alive).count() as i32;
        let m = &mut self.mechs[idx];
        if !m.alive {
            return;
        }
        m.alive = false;
        m.death_tick = self.tick;
        let p = m.pos;
        let cubes = m.cubes;
        let ws: Vec<WState> = m.weapons.iter().flatten().cloned().collect();
        let _ = alive_before;
        for w in ws {
            if w.kind != WK::Gun {
                let id = self.next_id;
                self.next_id += 1;
                let pp = [p[0] + self.rng.range(-2.0, 2.0), p[1] + self.rng.range(-2.0, 2.0), p[2]];
                self.pickups.push(PickupS { id, kind: w.kind, pos: pp, ammo: w.ammo });
            }
        }
        for _ in 0..((cubes + self.cfg.death_drop_extra) as i32) {
            self.spawn_cube(p);
        }
        if src >= 0 && (src as usize) != idx {
            self.mechs[src as usize].kills += 1;
            let tid = self.mechs[idx].id as i64;
            self.mechs[src as usize].events.push(Event { typ: "kill".into(), target: tid, ..Default::default() });
        }
        self.sound("death", p, idx as i32);
    }

    fn explode(&mut self, p: &Proj, at: V3, direct: Option<(u8, usize)>) {
        let owner = p.owner;
        if let Some((k, j)) = direct {
            self.damage(owner, k, j, p.dmg, "mortar", at, None);
        }
        // splash on mechs
        for j in 0..self.mechs.len() {
            if !self.mechs[j].alive {
                continue;
            }
            if let Some((0, dj)) = direct {
                if dj == j {
                    continue;
                }
            }
            let m = &self.mechs[j];
            let c = [m.pos[0], m.pos[1], m.pos[2] + m.stats.height * 0.5];
            let d = (d3(&c, &at) - m.stats.radius).max(0.0);
            if d > p.splash_r {
                continue;
            }
            if !self.world.los(at, c) && !self.world.los([at[0], at[1], at[2] + 0.5], [m.pos[0], m.pos[1], m.pos[2] + 1.0]) {
                continue;
            }
            let f = 1.0 - (1.0 - p.edge) * (d / p.splash_r);
            let mut dmg = p.splash * f;
            if m.id == owner {
                dmg *= m.stats.self_damage_mult;
            }
            // knockback & drain
            let (kx, ky) = norm2(m.pos[0] - at[0], m.pos[1] - at[1]);
            {
                let mm = &mut self.mechs[j];
                mm.vel[0] += kx * p.knock * f;
                mm.vel[1] += ky * p.knock * f;
                mm.vel[2] += p.knock_up * f;
                mm.grounded = false;
                mm.energy = (mm.energy - p.drain * f).max(0.0);
                if owner != mm.id {
                    mm.events.push(Event { typ: "effect".into(), effect: "concuss".into(), by: Some(owner), ..Default::default() });
                }
            }
            if self.mechs[j].id == owner {
                // self damage without cube multiplier: emulate by dividing
                let mult = 1.0 + self.cfg.damage_per_cube * self.mechs[j].cubes.min(self.cfg.stat_cap);
                self.damage(owner, 0, j, dmg / mult, "mortar", at, None);
            } else {
                self.damage(owner, 0, j, dmg, "mortar", at, None);
            }
        }
        for j in 0..self.npcs.len() {
            if !self.npcs[j].alive {
                continue;
            }
            if let Some((1, dj)) = direct {
                if dj == j {
                    continue;
                }
            }
            let d = d3(&self.npcs[j].pos, &at);
            if d < p.splash_r + self.cfg.npc_radius {
                let f = 1.0 - (1.0 - p.edge) * (d / (p.splash_r + 1.8)).min(1.0);
                self.damage(owner, 1, j, p.splash * f, "mortar", at, None);
            }
        }
        for j in 0..self.crates.len() {
            if self.crates[j].hp <= 0.0 {
                continue;
            }
            if let Some((2, dj)) = direct {
                if dj == j {
                    continue;
                }
            }
            let d = d3(&self.crates[j].pos, &at);
            if d < p.splash_r + 1.4 {
                let f = 1.0 - (1.0 - p.edge) * (d / (p.splash_r + 1.4)).min(1.0);
                self.damage(owner, 2, j, p.splash * f, "mortar", at, None);
            }
        }
        self.sound("explosion", at, owner);
    }

    fn fire_weapons(&mut self, i: usize, dt: f64) {
        let now = self.t;
        for s in 0..2 {
            let fire = self.mechs[i].cmd.fire[s];
            let w = match self.mechs[i].weapons[s].clone() {
                Some(w) => w,
                None => continue,
            };
            let mut w = w;
            w.cooldown = (w.cooldown - dt).max(0.0);
            w.equip = (w.equip - dt).max(0.0);
            let ws = self.mechs[i].wstats[w.kind as usize].clone();
            let m = &self.mechs[i];
            let muzzle_h = m.stats.muzzle_height;
            let (ay, ap) = (m.aim[0], m.aim[1]);
            let dirv = dir(ay, ap);
            let base = [m.pos[0], m.pos[1], m.pos[2] + muzzle_h];
            // shot origin: radius + 0.5 out, less if a wall is closer
            let out = m.stats.radius + 0.5;
            let tip = add(base, mul(dirv, out));
            let origin = match self.world.ray_block(base, tip) {
                Some(f) => add(base, mul(dirv, out * f * 0.9)),
                None => tip,
            };
            let blocked_barrel = self.world.ray_block(base, tip).is_some();
            match w.kind {
                WK::Gun => {
                    if w.reload > 0.0 {
                        w.reload -= dt;
                        if w.reload <= 0.0 {
                            w.reload = 0.0;
                            w.ammo = ws.mag;
                        }
                    } else if self.mechs[i].cmd.reload && w.ammo < ws.mag {
                        w.reload = ws.reload;
                        self.sound("reload", base, i as i32);
                    } else if fire && w.cooldown <= 0.0 && w.ammo > 0.0 && w.equip <= 0.0 {
                        w.ammo -= 1.0;
                        self.mechs[i].gun_shots += 1;
                        w.cooldown += ws.cooldown;
                        if w.cooldown < 0.0 {
                            w.cooldown = 0.0;
                        }
                        let sp = ws.spread_deg.to_radians();
                        let yy = ay + self.rng.gauss() * sp * 0.5;
                        let pp = ap + self.rng.gauss() * sp * 0.5;
                        let v = mul(dir(yy, pp), ws.speed);
                        let id = self.next_id;
                        self.next_id += 1;
                        if !blocked_barrel {
                            self.projs.push(Proj {
                                id,
                                kind: 0,
                                owner: i as i32,
                                pos: origin,
                                vel: v,
                                dmg: ws.damage,
                                travelled: 0.0,
                                range: ws.range,
                                ttl: 5.0,
                                splash: 0.0,
                                splash_r: 0.0,
                                edge: 0.0,
                                knock: 0.0,
                                knock_up: 0.0,
                                drain: 0.0,
                            });
                        }
                        self.sound("gun", base, i as i32);
                        if w.ammo <= 0.0 {
                            w.reload = ws.reload;
                        }
                    }
                }
                WK::Mortar => {
                    if fire && w.cooldown <= 0.0 && w.equip <= 0.0 {
                        w.cooldown = ws.cooldown;
                        let tube = [m.pos[0], m.pos[1], m.pos[2] + m.stats.height + 0.5];
                        let o2 = add(tube, mul(dirv, out));
                        let v = mul(dirv, ws.speed);
                        let id = self.next_id;
                        self.next_id += 1;
                        self.projs.push(Proj {
                            id,
                            kind: 1,
                            owner: i as i32,
                            pos: o2,
                            vel: v,
                            dmg: ws.damage,
                            travelled: 0.0,
                            range: 1e9,
                            ttl: ws.ttl,
                            splash: ws.splash,
                            splash_r: ws.splash_radius,
                            edge: ws.splash_edge,
                            knock: ws.knock,
                            knock_up: ws.knock_up,
                            drain: ws.drain,
                        });
                        self.sound("mortar", base, i as i32);
                    }
                }
                WK::Laser => {
                    let shorted = now < self.mechs[i].short_until;
                    let mut release = false;
                    if w.charge > 0.0 {
                        if shorted {
                            w.charge = 0.0;
                            w.hold = 0.0;
                        } else {
                            w.charge = (w.charge + dt).min(ws.charge);
                            if w.charge >= ws.charge {
                                w.hold += dt;
                                if w.hold >= ws.hold_s {
                                    release = true;
                                }
                            }
                            if !fire && w.charge >= ws.min_charge {
                                release = true;
                            }
                        }
                    } else if fire && w.cooldown <= 0.0 && w.equip <= 0.0 && !shorted {
                        w.charge = dt;
                        w.hold = 0.0;
                        self.sound("laser_charge", base, i as i32);
                    }
                    if release {
                        let frac = ws.min_frac + (1.0 - ws.min_frac) * (w.charge / ws.charge).min(1.0);
                        let dmg = ws.damage * frac;
                        w.charge = 0.0;
                        w.hold = 0.0;
                        w.cooldown = ws.cooldown;
                        let end = add(origin, mul(dirv, ws.range));
                        let wall = self.world.ray_block(origin, end).unwrap_or(1.0);
                        let wend = add(origin, mul(sub(end, origin), wall));
                        let mut skip: Vec<i64> = vec![];
                        let mut pierce = ws.pierce as i32;
                        let mut last = wend;
                        loop {
                            match self.ray_hit_actor(origin, wend, i as i32, 0.0, &skip) {
                                Some((tt, k, j)) => {
                                    let hitp = add(origin, mul(sub(wend, origin), tt));
                                    last = hitp;
                                    let tid = match k {
                                        0 => self.mechs[j].id as i64,
                                        1 => self.npcs[j].id as i64,
                                        _ => self.crates[j].id,
                                    };
                                    skip.push(tid);
                                    self.damage(i as i32, k, j, dmg, "laser", origin, Some("cookoff"));
                                    if pierce <= 0 {
                                        break;
                                    }
                                    pierce -= 1;
                                }
                                None => break,
                            }
                        }
                        self.beams.push((i as i32, true, vec![origin, last], now));
                        self.sound("laser", base, i as i32);
                    }
                }
                WK::Lightning => {
                    if fire && w.cooldown <= 0.0 && w.equip <= 0.0 && self.mechs[i].energy >= ws.energy {
                        w.cooldown = ws.cooldown;
                        self.mechs[i].energy -= ws.energy;
                        self.mechs[i].last_spend = now;
                        // nearest target in cone with LOS
                        let cone = (ws.cone_deg * 0.5).to_radians();
                        let mut cands: Vec<(f64, u8, usize, V3)> = Vec::new();
                        for (j, t) in self.mechs.iter().enumerate() {
                            if j == i || !t.alive {
                                continue;
                            }
                            let c = [t.pos[0], t.pos[1], t.pos[2] + t.stats.height * 0.5];
                            cands.push((0.0, 0, j, c));
                        }
                        for (j, n) in self.npcs.iter().enumerate() {
                            if n.alive {
                                cands.push((0.0, 1, j, [n.pos[0], n.pos[1], n.pos[2] + 1.75]));
                            }
                        }
                        for (j, c) in self.crates.iter().enumerate() {
                            if c.hp > 0.0 {
                                cands.push((0.0, 2, j, [c.pos[0], c.pos[1], c.pos[2] + 1.2]));
                            }
                        }
                        let mut first: Option<(f64, u8, usize, V3)> = None;
                        for c in &cands {
                            let d = d3(&base, &c.3);
                            if d > ws.range {
                                continue;
                            }
                            if wrap(yaw_to(&base, &c.3) - ay).abs() > cone {
                                continue;
                            }
                            if !self.world.los(origin, c.3) {
                                continue;
                            }
                            if first.map_or(true, |f| d < f.0) {
                                first = Some((d, c.1, c.2, c.3));
                            }
                        }
                        let mut path = vec![origin];
                        if let Some(f) = first {
                            let mut dmg = ws.damage;
                            let mut hit: Vec<(u8, usize)> = vec![(f.1, f.2)];
                            let mut cur = f.3;
                            path.push(cur);
                            self.damage(i as i32, f.1, f.2, dmg, "lightning", origin, None);
                            if f.1 == 0 && self.mechs[f.2].weapons.iter().flatten().any(|w| w.kind == WK::Laser) {
                                self.mechs[f.2].short_until = now + ws.short_s;
                            }
                            for _ in 0..(ws.chain as i32) {
                                dmg *= ws.chain_falloff;
                                let mut nb: Option<(f64, u8, usize, V3)> = None;
                                for c in &cands {
                                    if hit.contains(&(c.1, c.2)) {
                                        continue;
                                    }
                                    if c.1 == 0 && c.2 == i {
                                        continue;
                                    }
                                    let d = d3(&cur, &c.3);
                                    if d <= ws.chain_range && self.world.los(cur, c.3) && nb.map_or(true, |b| d < b.0) {
                                        nb = Some((d, c.1, c.2, c.3));
                                    }
                                }
                                match nb {
                                    Some(b) => {
                                        hit.push((b.1, b.2));
                                        cur = b.3;
                                        path.push(cur);
                                        self.damage(i as i32, b.1, b.2, dmg, "lightning", origin, None);
                                    }
                                    None => break,
                                }
                            }
                        } else {
                            let end = add(origin, mul(dirv, ws.range));
                            let f = self.world.ray_block(origin, end).unwrap_or(1.0);
                            path.push(add(origin, mul(sub(end, origin), f)));
                        }
                        self.beams.push((i as i32, false, path, now));
                        self.sound("lightning", base, i as i32);
                    }
                }
            }
            if self.mechs[i].alive {
                // weapon may have been replaced during damage callbacks (not possible), store back
                if let Some(cur) = &self.mechs[i].weapons[s] {
                    if cur.kind == w.kind {
                        self.mechs[i].weapons[s] = Some(w);
                    }
                }
            }
        }
    }

    fn step_projectiles(&mut self, dt: f64) {
        let mut k = 0;
        let g = self.cfg.w(WK::Mortar).gravity;
        while k < self.projs.len() {
            let mut p = self.projs[k].clone();
            let a = p.pos;
            if p.kind == 1 {
                p.vel[2] -= g * dt;
            }
            let b = add(a, mul(p.vel, dt));
            let seg = d3(&a, &b);
            p.ttl -= dt;
            let wall = self.world.ray_block(a, b);
            let rad = if p.kind == 0 { self.cfg.gun_radius } else { 0.3 };
            let owner = if p.kind == 2 { -1000 } else { p.owner };
            let actor = self.ray_hit_actor(a, b, owner, rad, &[]);
            let actor = match actor {
                Some((t, kk, j)) if p.kind == 2 && kk != 0 => {
                    let _ = (t, j);
                    None
                }
                x => x,
            };
            let hit_wall_t = wall.unwrap_or(2.0);
            let mut remove = false;
            if let Some((t, kk, j)) = actor {
                if t <= hit_wall_t {
                    let at = add(a, mul(sub(b, a), t));
                    match p.kind {
                        0 => {
                            if kk == 0 && p.owner >= 0 {
                                self.mechs[p.owner as usize].gun_hits += 1;
                            }
                            self.damage(p.owner, kk, j, p.dmg, "gun", a, None)
                        }
                        1 => self.explode(&p, at, Some((kk, j))),
                        _ => {
                            if kk == 0 {
                                let dm = self.cfg.npc_damage;
                                self.damage(-1, 0, j, dm, "npc", a, None);
                            }
                        }
                    }
                    remove = true;
                }
            }
            if !remove && wall.is_some() {
                if p.kind == 1 {
                    let at = add(a, mul(sub(b, a), hit_wall_t * 0.98));
                    self.explode(&p, at, None);
                }
                remove = true;
            }
            p.travelled += seg;
            if !remove && (p.travelled > p.range || p.ttl <= 0.0) {
                if p.kind == 1 {
                    self.explode(&p, b, None);
                }
                remove = true;
            }
            if remove {
                self.projs.swap_remove(k);
            } else {
                p.pos = b;
                self.projs[k] = p;
                k += 1;
            }
        }
    }

    fn step_npcs(&mut self, dt: f64) {
        for k in 0..self.npcs.len() {
            if !self.npcs[k].alive {
                continue;
            }
            let n = self.npcs[k].clone();
            let mut n2 = n.clone();
            n2.cooldown = (n2.cooldown - dt).max(0.0);
            // target validity
            if let Some(tid) = n2.target {
                let tm = &self.mechs[tid as usize];
                if !tm.alive || d2(&tm.pos, &n2.pos) > self.cfg.npc_deaggro || d2(&n2.pos, &n2.home) > self.cfg.npc_chase_leash {
                    n2.target = None;
                }
            }
            if n2.target.is_none() && d2(&n2.pos, &n2.home) <= self.cfg.npc_leash {
                let mut best: Option<(f64, i32)> = None;
                for m in &self.mechs {
                    if !m.alive {
                        continue;
                    }
                    let d = d3(&m.pos, &n2.pos);
                    if d < self.cfg.npc_aggro && self.world.los([n2.pos[0], n2.pos[1], n2.pos[2] + 3.0], [m.pos[0], m.pos[1], m.pos[2] + 4.5]) && best.map_or(true, |b| d < b.0) {
                        best = Some((d, m.id));
                    }
                }
                n2.target = best.map(|b| b.1);
            }
            let mut goal = n2.wander;
            if let Some(tid) = n2.target {
                let tm = &self.mechs[tid as usize];
                let tp = tm.pos;
                n2.aim = yaw_to(&n2.pos, &tp);
                if d2(&tp, &n2.pos) > 25.0 {
                    goal = (tp[0], tp[1]);
                } else {
                    goal = (n2.pos[0], n2.pos[1]);
                }
                let eye = [n2.pos[0], n2.pos[1], n2.pos[2] + 3.0];
                let tc = [tp[0], tp[1], tp[2] + tm.stats.height * 0.5];
                if n2.cooldown <= 0.0 && self.world.los(eye, tc) {
                    n2.cooldown = self.cfg.npc_cooldown;
                    let sp = self.cfg.npc_spread_deg.to_radians();
                    let tt = d3(&eye, &tc) / self.cfg.npc_proj_speed;
                    let lead = add(tc, mul(tm.vel, tt * 0.5));
                    let (yy, pp) = angles(sub(lead, eye));
                    let v = mul(dir(yy + self.rng.gauss() * sp * 0.5, pp + self.rng.gauss() * sp * 0.5), self.cfg.npc_proj_speed);
                    let id = self.next_id;
                    self.next_id += 1;
                    let origin = add(eye, mul(dir(yy, pp), 2.3));
                    self.projs.push(Proj {
                        id,
                        kind: 2,
                        owner: n2.id,
                        pos: origin,
                        vel: v,
                        dmg: self.cfg.npc_damage,
                        travelled: 0.0,
                        range: 200.0,
                        ttl: 3.0,
                        splash: 0.0,
                        splash_r: 0.0,
                        edge: 0.0,
                        knock: 0.0,
                        knock_up: 0.0,
                        drain: 0.0,
                    });
                    self.sound("npc_gun", eye, -1);
                }
            } else {
                n2.wander_t -= dt;
                if n2.wander_t <= 0.0 || d2(&[n2.pos[0], n2.pos[1]], &[n2.wander.0, n2.wander.1]) < 2.0 {
                    let a = self.rng.range(-PI, PI);
                    let r = self.rng.f() * self.cfg.npc_leash;
                    n2.wander = (n2.home[0] + r * a.cos(), n2.home[1] + r * a.sin());
                    n2.wander_t = self.rng.range(3.0, 8.0);
                }
                goal = n2.wander;
            }
            let (dx, dy) = norm2(goal.0 - n2.pos[0], goal.1 - n2.pos[1]);
            let nx = n2.pos[0] + dx * self.cfg.npc_speed * dt;
            let ny = n2.pos[1] + dy * self.cfg.npc_speed * dt;
            if let Some(kk) = self.nav.idx(nx, ny) {
                if self.nav.stand[kk] && (self.nav.hgt[kk] as f64 - n2.pos[2]).abs() < 1.5 {
                    n2.pos = [nx, ny, self.world.surface(nx, ny)];
                }
            }
            self.npcs[k] = n2;
        }
    }

    fn sees_point(&self, i: usize, p: V3, near: f64, range: f64, fov: f64, mem: bool) -> bool {
        let m = &self.mechs[i];
        let eye = [m.pos[0], m.pos[1], m.pos[2] + m.stats.eye_height];
        let d = d2(&eye, &p);
        let in_near = d <= near;
        if !in_near {
            if d > range {
                return false;
            }
            if wrap(yaw_to(&eye, &p) - m.aim[0]).abs() > fov * 0.5 {
                return false;
            }
        }
        let _ = mem;
        if !self.world.los(eye, p) {
            return false;
        }
        if !in_near {
            // canopy fade
            let tb = self.world.tree_block(eye, p);
            if tb > 7.0 {
                return false;
            }
        }
        true
    }

    fn observe(&mut self, i: usize) -> Obs {
        let now = self.t;
        let n = self.mechs.len();
        let me = self.mechs[i].clone();
        let near = self.cfg.near_range + me.stats.near_range_bonus;
        let range = self.cfg.vis_range + me.stats.vision_bonus;
        let fov = self.cfg.fov_deg.to_radians();
        let mut mechs = Vec::new();
        for j in 0..n {
            if j == i || !self.mechs[j].alive {
                continue;
            }
            let t = self.mechs[j].clone();
            if d2(&t.pos, &me.pos) > range + 50.0 && now - self.mechs[i].seen[j] > self.cfg.memory_s {
                continue;
            }
            let c = [t.pos[0], t.pos[1], t.pos[2] + t.stats.height * 0.5];
            let top = [t.pos[0], t.pos[1], t.pos[2] + t.stats.height];
            let vis = self.sees_point(i, c, near, range, fov, false) || self.sees_point(i, top, near, range, fov, false);
            if vis {
                self.mechs[i].seen[j] = now;
            }
            if vis || now - self.mechs[i].seen[j] <= self.cfg.memory_s {
                let charging = t.weapons.iter().flatten().any(|w| w.kind == WK::Laser && w.charge > 0.0);
                mechs.push(Actor {
                    id: t.id,
                    is_mech: true,
                    pos: t.pos,
                    vel: t.vel,
                    aim: t.aim,
                    hp: t.hp.max(0.0),
                    max_hp: t.stats.hp,
                    cubes: t.cubes,
                    weapons: t.weapons.iter().flatten().map(|w| w.kind).collect(),
                    charging,
                });
            }
        }
        for (k, np) in self.npcs.iter().enumerate() {
            if !np.alive {
                continue;
            }
            if d2(&np.pos, &me.pos) > range + 10.0 {
                continue;
            }
            let c = [np.pos[0], np.pos[1], np.pos[2] + 1.75];
            let vis = self.sees_point(i, c, near, range, fov, false);
            if k < self.mechs[i].seen_npc.len() {
                if vis {
                    self.mechs[i].seen_npc[k] = now;
                }
                if vis || now - self.mechs[i].seen_npc[k] <= self.cfg.memory_s {
                    mechs.push(Actor { id: np.id, is_mech: false, pos: np.pos, vel: np.vel, aim: [np.aim, 0.0], hp: np.hp, max_hp: self.cfg.npc_hp, cubes: 0.0, weapons: vec![], charging: false });
                }
            }
        }
        let projectiles = self
            .projs
            .iter()
            .filter(|p| p.owner != i as i32 || p.kind == 2)
            .filter(|p| d2(&p.pos, &me.pos) < range && self.sees_point(i, p.pos, near, range, fov, false))
            .map(|p| crate::proto::Proj { id: p.id, kind: p.kind, pos: p.pos, vel: p.vel })
            .collect();
        let crates = self
            .crates
            .iter()
            .filter(|c| c.hp > 0.0 && d2(&c.pos, &me.pos) < range)
            .filter(|c| self.sees_point(i, [c.pos[0], c.pos[1], c.pos[2] + 1.2], near, range, fov, false))
            .map(|c| Item { id: c.id, pos: c.pos, hp: c.hp })
            .collect();
        let cubes = self
            .cubes
            .iter()
            .filter(|c| d2(&c.pos, &me.pos) < range)
            .filter(|c| self.sees_point(i, [c.pos[0], c.pos[1], c.pos[2] + 0.5], near, range, fov, false))
            .cloned()
            .collect();
        let pickups = self
            .pickups
            .iter()
            .filter(|c| d2(&c.pos, &me.pos) < range)
            .filter(|c| self.sees_point(i, [c.pos[0], c.pos[1], c.pos[2] + 0.5], near, range, fov, false))
            .map(|c| Pickup { id: c.id, kind: c.kind, pos: c.pos })
            .collect();
        // sounds since last decision
        let hearing = me.stats.hearing;
        let mut sounds = Vec::new();
        let sounds_src: Vec<SoundS> = self.sounds.iter().filter(|s| now - s.t < 0.1 + 1e-9 && s.src != i as i32).cloned().collect();
        for s in sounds_src {
            let d = d2(&s.pos, &me.pos);
            if d > self.cfg.sound_range(s.kind) {
                continue;
            }
            let b = yaw_to(&me.pos, &s.pos) + self.rng.gauss() * self.cfg.bearing_noise_deg.to_radians() * hearing;
            sounds.push(Sound {
                kind: s.kind.to_string(),
                bearing: wrap(b),
                dist: if d < self.cfg.snd_near {
                    0
                } else if d < self.cfg.snd_mid {
                    1
                } else {
                    2
                },
                age: now - s.t,
            });
        }
        let beams = self
            .beams
            .iter()
            .filter(|b| now - b.3 < 0.1 + 1e-9 && b.0 != i as i32)
            .filter(|b| b.2.iter().any(|p| d2(p, &me.pos) < range && self.world.los([me.pos[0], me.pos[1], me.pos[2] + me.stats.eye_height], *p)))
            .map(|b| Beam { by: b.0, laser: b.1, path: b.2.clone(), age: now - b.3 })
            .collect();
        let events = std::mem::take(&mut self.mechs[i].events);
        let drops = self
            .drops
            .iter()
            .filter(|d| !d.taken)
            .map(|d| Drop {
                id: d.id,
                pos: d.pos,
                radius: self.cfg.drop_radius,
                lands_in: (d.land_t - now).max(0.0),
                capture_s: self.cfg.drop_capture_s,
                progress: d.progress,
                holder: d.holder,
            })
            .collect();
        let offer = me.offers.first().map(|o| Offer { id: o.0, choices: o.1.clone(), expires: o.2 });
        let (zc, zr, _, _, _) = self.zone_now();
        let in_zone = d2(&me.pos, &zc) <= zr;
        let weapons = [0, 1].map(|s| {
            me.weapons[s].as_ref().map(|w| Weapon {
                kind: w.kind,
                cooldown: w.cooldown.max(w.equip),
                ammo: w.ammo,
                reload: w.reload.max(0.0),
                charge: w.charge,
                stats: me.wstats[w.kind as usize].clone(),
            })
        });
        Obs {
            tick: self.tick,
            t: (now * 100.0).round() / 100.0,
            alive: self.mechs.iter().filter(|m| m.alive).count() as i32,
            me: SelfObs {
                id: me.id,
                pos: me.pos,
                vel: me.vel,
                aim: me.aim,
                hp: me.hp,
                max_hp: me.stats.hp,
                energy: me.energy,
                max_energy: me.stats.energy,
                grounded: me.grounded,
                cubes: me.cubes,
                upgrades: me.upgrades.clone(),
                weapons,
                in_zone,
                stats: me.stats.clone(),
            },
            mechs,
            projectiles,
            crates,
            cubes,
            pickups,
            sounds,
            beams,
            events,
            drops,
            zone: self.zone_obs(),
            offer,
        }
    }

    fn apply_action(&mut self, i: usize, a: &Action) {
        let now = self.t;
        // continuous parts
        let mut cmd = a.clone();
        cmd.mv = {
            let l = (a.mv[0] * a.mv[0] + a.mv[1] * a.mv[1]).sqrt();
            if l > 1.0 {
                [a.mv[0] / l, a.mv[1] / l]
            } else {
                a.mv
            }
        };
        if cmd.aim.is_none() {
            cmd.aim = self.mechs[i].cmd.aim;
        }
        self.mechs[i].cmd = cmd;
        self.mechs[i].missed = 0.0;
        // offer pick
        if let Some(p) = a.pick {
            if let Some(of) = self.mechs[i].offers.first().cloned() {
                if p < of.1.len() {
                    self.mechs[i].offers.remove(0);
                    let uid = of.1[p].clone();
                    self.apply_upgrade(i, &uid);
                }
            }
        }
        // take / drop
        if let Some(tid) = a.take {
            let me_pos = self.mechs[i].pos;
            let pk = self.pickups.iter().position(|p| p.id == tid);
            let mut reason: Option<&str> = None;
            match pk {
                None => reason = Some("gone"),
                Some(pi) => {
                    let p = self.pickups[pi].clone();
                    let dh = d2(&p.pos, &me_pos);
                    if dh > self.cfg.pickup_radius || (p.pos[2] - me_pos[2]).abs() > 4.0 {
                        reason = Some("range");
                    } else if self.mechs[i].weapons.iter().flatten().any(|w| w.kind == p.kind)
                        && !(a.drop.map_or(false, |d| self.mechs[i].weapons[d.min(1)].as_ref().map_or(false, |w| w.kind == p.kind)))
                    {
                        reason = Some("have");
                    } else {
                        let slot = match a.drop {
                            Some(d) => Some(d.min(1)),
                            None => self.mechs[i].weapons.iter().position(|w| w.is_none()),
                        };
                        match slot {
                            None => reason = Some("full"),
                            Some(s) => {
                                if self.mechs[i].weapons[s].as_ref().map_or(false, |w| w.kind == WK::Laser && w.charge > 0.0) {
                                    reason = Some("busy");
                                } else {
                                    // drop old
                                    if let Some(old) = self.mechs[i].weapons[s].take() {
                                        let id = self.next_id;
                                        self.next_id += 1;
                                        self.pickups.push(PickupS { id, kind: old.kind, pos: me_pos, ammo: old.ammo });
                                        self.mechs[i].events.push(Event { typ: "dropped".into(), id, kind: old.kind.name().into(), slot: s as i32, ..Default::default() });
                                    }
                                    self.pickups.remove(pi);
                                    let ws = &self.mechs[i].wstats[p.kind as usize];
                                    let ammo = if p.kind == WK::Gun { if p.ammo > 0.0 { p.ammo } else { ws.mag } } else { 0.0 };
                                    let equip = self.cfg.equip_s.max(ws.cooldown);
                                    self.mechs[i].weapons[s] = Some(WState { kind: p.kind, cooldown: 0.0, ammo, reload: 0.0, charge: 0.0, hold: 0.0, equip });
                                    self.mechs[i].events.push(Event { typ: "took".into(), id: p.id, kind: p.kind.name().into(), slot: s as i32, ..Default::default() });
                                }
                            }
                        }
                    }
                }
            }
            if let Some(r) = reason {
                self.mechs[i].events.push(Event { typ: "take_failed".into(), id: tid, reason: r.into(), ..Default::default() });
            }
        } else if let Some(d) = a.drop {
            let s = d.min(1);
            let ok = self.mechs[i].weapons[s].as_ref().map_or(false, |w| !(w.kind == WK::Laser && w.charge > 0.0));
            if ok {
                let old = self.mechs[i].weapons[s].take().unwrap();
                let id = self.next_id;
                self.next_id += 1;
                let p = self.mechs[i].pos;
                self.pickups.push(PickupS { id, kind: old.kind, pos: p, ammo: old.ammo });
            }
        }
        let _ = now;
    }

    pub fn run(&mut self, agents: &mut [Box<dyn Agent>]) -> Vec<i32> {
        let n = self.mechs.len();
        let dt = 1.0 / self.cfg.tick_hz;
        let decide = self.cfg.decide_every as i64;
        loop {
            // decisions
            if self.tick % decide == 0 {
                if let Some(intel) = &self.intel {
                    let mut it = intel.borrow_mut();
                    it.t = self.t;
                    it.prey = None;
                    let prey = (0..n).find(|&j| !self.squad[j] && self.mechs[j].alive);
                    if let Some(p) = prey {
                        if self.prey_known || self.t - self.squad_seen[p] <= self.cfg.memory_s {
                            it.prey = Some((p as i32, self.mechs[p].pos, self.mechs[p].vel));
                        }
                    }
                    let alive_sq: Vec<usize> = (0..n).filter(|&j| self.squad[j] && self.mechs[j].alive).collect();
                    let total_sq = self.squad.iter().filter(|&&b| b).count();
                    let killed = total_sq - alive_sq.len();
                    let nh = if self.t >= self.all_hunt_t { alive_sq.len() } else { (1 + killed).min(alive_sq.len()) };
                    let mut ord = alive_sq.clone();
                    ord.sort_by_key(|&j| self.rank[j]);
                    it.hunters = ord.iter().take(nh).map(|&j| j as i32).collect();
                }
                for i in 0..n {
                    if !self.mechs[i].alive {
                        continue;
                    }
                    // offers timeout
                    if let Some(of) = self.mechs[i].offers.first().cloned() {
                        if self.t >= of.2 {
                            self.mechs[i].offers.remove(0);
                            if let Some(u) = of.1.first() {
                                self.apply_upgrade(i, u);
                            }
                        }
                    }
                    let o = self.observe(i);
                    let a = agents[i].act(&o);
                    // one-shot parts reset each decision
                    self.mechs[i].cmd.fire = [false, false];
                    self.apply_action(i, &a);
                }
                // squad vision union (used next round)
                for j in 0..n {
                    let mut last = self.squad_seen[j];
                    for i in 0..n {
                        if self.squad[i] && self.mechs[i].alive && i != j {
                            last = last.max(self.mechs[i].seen[j]);
                        }
                    }
                    self.squad_seen[j] = last;
                }
            }
            // physics
            for i in 0..n {
                if self.mechs[i].alive {
                    self.move_mech(i, dt);
                }
            }
            // separate overlapping mechs
            for i in 0..n {
                for j in (i + 1)..n {
                    if !self.mechs[i].alive || !self.mechs[j].alive {
                        continue;
                    }
                    let (a, b) = (self.mechs[i].pos, self.mechs[j].pos);
                    let rr = self.mechs[i].stats.radius + self.mechs[j].stats.radius;
                    let d = d2(&a, &b);
                    if d < rr && (a[2] - b[2]).abs() < 8.0 {
                        let (nx, ny) = if d > 1e-6 { ((b[0] - a[0]) / d, (b[1] - a[1]) / d) } else { (1.0, 0.0) };
                        let push = (rr - d) * 0.5;
                        let pa = [a[0] - nx * push, a[1] - ny * push];
                        let pb = [b[0] + nx * push, b[1] + ny * push];
                        let (ri, rj) = (self.mechs[i].stats.radius, self.mechs[j].stats.radius);
                        if !self.blocked_at(pa[0], pa[1], a[2], ri, 9.0, 1.2) {
                            self.mechs[i].pos[0] = pa[0];
                            self.mechs[i].pos[1] = pa[1];
                        }
                        if !self.blocked_at(pb[0], pb[1], b[2], rj, 9.0, 1.2) {
                            self.mechs[j].pos[0] = pb[0];
                            self.mechs[j].pos[1] = pb[1];
                        }
                    }
                }
            }
            // weapons
            for i in 0..n {
                if self.mechs[i].alive {
                    self.fire_weapons(i, dt);
                }
            }
            self.step_projectiles(dt);
            self.step_npcs(dt);
            // pickups of cubes
            let mut ci = 0;
            while ci < self.cubes.len() {
                let c = self.cubes[ci].pos;
                let mut best: Option<(f64, usize)> = None;
                for (j, m) in self.mechs.iter().enumerate() {
                    if !m.alive {
                        continue;
                    }
                    let d = d2(&m.pos, &c);
                    if d <= self.cfg.cube_pickup_radius + m.stats.radius * 0.0 && (m.pos[2] - c[2]).abs() <= 4.0 && best.map_or(true, |b| d < b.0) {
                        best = Some((d, j));
                    }
                }
                if let Some((_, j)) = best {
                    self.cubes.swap_remove(ci);
                    self.give_cubes(j, 1.0);
                    let c2 = self.mechs[j].cubes;
                    self.mechs[j].events.push(Event { typ: "pickup".into(), cubes: c2, ..Default::default() });
                } else {
                    ci += 1;
                }
            }
            // zone damage & regen
            let (zc, zr, _, _, dps) = self.zone_now();
            let now = self.t;
            for i in 0..n {
                if !self.mechs[i].alive {
                    continue;
                }
                let inz = d2(&self.mechs[i].pos, &zc) <= zr;
                if !inz {
                    let m = &mut self.mechs[i];
                    m.hp -= dps * dt;
                    m.last_combat = now;
                    m.zone_out_t = now;
                    if self.tick % decide == 0 {
                        m.events.push(Event { typ: "damaged".into(), amount: dps * dt * 2.0, weapon: "zone".into(), ..Default::default() });
                    }
                    if m.hp <= 0.0 {
                        self.kill_mech(i, -1);
                    }
                } else {
                    let m = &mut self.mechs[i];
                    if now - m.last_combat >= m.stats.regen_delay && m.hp < m.stats.hp {
                        m.hp = (m.hp + m.stats.regen_per_s * dt).min(m.stats.hp);
                    }
                }
            }
            // supply drops
            for (k, &ct) in self.cfg.drop_times.clone().iter().enumerate() {
                if (self.t - ct).abs() < dt * 0.5 && self.drops.len() == k {
                    let c = self.next_c;
                    let r = self.next_r * self.cfg.drop_spread;
                    let p = self.random_ground(c, r.max(5.0), 20.0).unwrap_or([c[0], c[1], self.world.ground(c[0], c[1])]);
                    self.drops.push(DropS { id: 20000 + k as i64, pos: p, call_t: ct, land_t: ct + self.cfg.drop_fall_s, holder: None, progress: 0.0, taken: false });
                }
            }
            for k in 0..self.drops.len() {
                if self.drops[k].taken || self.t < self.drops[k].land_t {
                    continue;
                }
                let p = self.drops[k].pos;
                let inside: Vec<usize> = (0..n).filter(|&j| self.mechs[j].alive && d2(&self.mechs[j].pos, &p) <= self.cfg.drop_radius && (self.mechs[j].pos[2] - p[2]).abs() < 6.0).collect();
                if inside.len() == 1 {
                    let j = inside[0];
                    if self.drops[k].holder != Some(j as i32) {
                        self.drops[k].holder = Some(j as i32);
                        self.drops[k].progress = 0.0;
                    }
                    self.drops[k].progress += dt;
                    if self.drops[k].progress >= self.cfg.drop_capture_s {
                        self.drops[k].taken = true;
                        let cubes = self.cfg.drop_cubes;
                        self.give_cubes(j, cubes);
                        let m = &mut self.mechs[j];
                        m.hp = m.stats.hp;
                        m.energy = m.stats.energy;
                        let id = self.drops[k].id;
                        let c2 = m.cubes;
                        m.events.push(Event { typ: "drop".into(), id, cubes: c2, ..Default::default() });
                    }
                } else if inside.is_empty() {
                    self.drops[k].holder = None;
                    self.drops[k].progress = 0.0;
                }
            }
            // zone phase advance
            if self.phase < self.cfg.zone_phases.len() {
                let ph = &self.cfg.zone_phases[self.phase];
                if self.t >= self.phase_start + ph.hold_s + ph.shrink_s {
                    self.zone_c = self.next_c;
                    self.zone_r = self.next_r;
                    self.phase_start += ph.hold_s + ph.shrink_s;
                    self.phase += 1;
                    self.pick_next_zone();
                }
            }
            // purge old sounds/beams
            let now = self.t;
            self.sounds.retain(|s| now - s.t < 0.2);
            self.beams.retain(|b| now - b.3 < 0.2);
            self.tick += 1;
            self.t = self.tick as f64 * dt;
            let alive = self.mechs.iter().filter(|m| m.alive).count();
            let prey_dead = self.intel.is_some() && (0..n).all(|j| self.squad[j] || !self.mechs[j].alive);
            if alive <= 1 || prey_dead || self.t >= self.max_t.min(self.cfg.duration_s) {
                break;
            }
        }
        // placements
        let mut place = vec![0i32; n];
        let mut alive: Vec<usize> = (0..n).filter(|&i| self.mechs[i].alive).collect();
        alive.sort_by(|&a, &b| self.mechs[b].hp.partial_cmp(&self.mechs[a].hp).unwrap());
        let mut p = 1;
        for (k, &i) in alive.iter().enumerate() {
            if k > 0 && (self.mechs[i].hp - self.mechs[alive[k - 1]].hp).abs() < 1e-9 {
                place[i] = place[alive[k - 1]];
            } else {
                place[i] = p;
            }
            p += 1;
        }
        let mut dead: Vec<usize> = (0..n).filter(|&i| !self.mechs[i].alive).collect();
        dead.sort_by(|&a, &b| self.mechs[b].death_tick.cmp(&self.mechs[a].death_tick));
        // same-tick deaths share a place: place = number alive before that tick
        for &i in &dead {
            let dt_i = self.mechs[i].death_tick;
            let later = (0..n).filter(|&j| self.mechs[j].alive || self.mechs[j].death_tick > dt_i).count();
            place[i] = later as i32 + 1;
        }
        self.place = place.clone();
        self.done = true;
        place
    }
}

impl MechS {
    fn cubes_prev(&self) -> f64 {
        self.cubes
    }
    fn prev_cubes_set(&mut self, _c: f64) {}
}
