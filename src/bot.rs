//! The bot: world model, decision making, aiming and firing.

use crate::geom::*;
use crate::nav::Nav;
use crate::proto::*;
use crate::world::World;
use std::f64::consts::PI;
use std::sync::Arc;

/// Tunable strategy parameters (the local simulator tunes these).
#[derive(Clone, Debug)]
pub struct Params {
    /// fight if my time-to-kill < their time-to-kill * this
    pub fight_ratio: f64,
    /// retreat when hp fraction below this and enemies engage
    pub retreat_hp: f64,
    /// fraction of next zone radius to hold at (from next center)
    pub hold_frac: f64,
    /// seconds of margin when rotating to the zone
    pub zone_margin_s: f64,
    /// weapon preferences [gun, mortar, laser, lightning]
    pub wpref: [f64; 4],
    /// distance at which we go for loot
    pub loot_range: f64,
    /// whether to hunt NPC drones
    pub hunt_npcs: bool,
    /// max distance to go for a supply drop
    pub drop_range: f64,
    /// walk below step-sound speed when not hurried
    pub silent: bool,
    pub reload_cover: bool,
    pub farm_hp: f64,
    pub new_pref: bool,
    /// avoid engaging enemies unless they engage us
    pub passive: bool,
    /// shoot back at drones that attack us (any mode, any hp)
    pub npc_fix: bool,
    /// value-based upgrade choice instead of the fixed order
    pub upg_v2: bool,
    /// with no reachable cover and a single attacker, fight instead of running in the open
    pub stand_no_cover: bool,
    /// turn to face unseen attackers that hit our rear
    pub face_unseen: bool,
    /// upgrade priority (ids)
    pub upgrade_order: Vec<&'static str>,
}

impl Default for Params {
    fn default() -> Params {
        Params {
            fight_ratio: 1.15,
            retreat_hp: 0.35,
            hold_frac: 0.5,
            zone_margin_s: 8.0,
            wpref: [1.0, 1.2, 3.0, 2.8],
            loot_range: 140.0,
            hunt_npcs: true,
            drop_range: 200.0,
            silent: true,
            reload_cover: true,
            farm_hp: 0.85,
            new_pref: true,
            passive: false,
            npc_fix: true,
            upg_v2: true,
            stand_no_cover: true,
            face_unseen: true,
            upgrade_order: vec![
                "armor",
                "plating",
                "nanites",
                "gun_rate",
                "gun_damage",
                "laser_damage",
                "laser_charge",
                "lightning_efficiency",
                "lightning_range",
                "servos",
                "lightning_chain",
                "gyros",
                "reactor",
                "capacitor",
                "jets",
                "gun_mag",
                "gun_velocity",
                "mortar_reload",
                "mortar_splash",
                "mortar_velocity",
                "sensors",
                "laser_pierce",
            ],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Track {
    pub id: i32,
    pub pos: V3,
    pub vel: V3,
    pub aim: [f64; 2],
    pub hp: f64,
    pub max_hp: f64,
    pub cubes: f64,
    pub weapons: Vec<WK>,
    pub charging: bool,
    pub seen_t: f64,
    pub first_t: f64,
    /// damage this mech dealt to me recently
    pub dmg_to_me: f64,
    pub last_hit_me_t: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Zone,
    Fight,
    Retreat,
    Loot,
    Hold,
}

pub struct Bot {
    pub cfg: Config,
    pub world: Arc<World>,
    pub nav: Nav,
    pub p: Params,
    pub rng: Rng,
    pub id: i32,
    pub tracks: Vec<Track>,
    path: Vec<(f64, f64)>,
    path_goal: (f64, f64),
    path_t: f64,
    strafe: f64,
    strafe_until: f64,
    jink: f64,
    jink_until: f64,
    stuck_t: f64,
    unstick_until: f64,
    last_dmg_t: f64,
    /// drones that shot us recently: (id or -1, bearing, time)
    npc_hurt: Vec<(i32, f64, f64)>,
    upg_taken: Vec<String>,
    cover_check: (f64, bool),
    /// hits from mechs we cannot see: (id, bearing, time, rear, amount)
    unseen: Vec<(i32, f64, f64, bool, f64)>,
    last_offer: i64,
    last_shot_t: f64,
    dmg_window: Vec<(f64, f64)>,
    pub mode: Mode,
    hold: Option<(f64, f64)>,
    hold_key: f64,
    blacklist: Vec<(i64, f64)>,
    loot_track: Option<(i64, f64, f64)>,
    target_id: Option<i32>,
    mem_pk: Vec<(i64, WK, V3)>,
    mem_cube: Vec<(i64, V3)>,
    mem_crate: Vec<(i64, V3)>,
    last_fight: bool,
    decide_lock: f64,
    last_t: f64,
    scan_yaw: f64,
    last_sound: Option<(f64, f64, f64)>, // bearing, dist estimate, time
    cover: Option<(f64, f64, f64)>,      // x, y, chosen time
    pub label: String,
    pub debug: bool,
}

pub fn wdps(k: WK, d: f64, cfg: &Config, ws: Option<&WStats>) -> f64 {
    let s = ws.unwrap_or(cfg.w(k));
    match k {
        WK::Gun => {
            if d > s.range {
                return 0.0;
            }
            let sustained = s.damage * s.mag / (s.mag * s.cooldown + s.reload);
            let hit = (1.0 - (d / 260.0)).clamp(0.25, 0.9);
            sustained * hit
        }
        WK::Laser => {
            if d > s.range {
                0.0
            } else {
                s.damage / (s.charge + s.cooldown) * 0.85
            }
        }
        WK::Lightning => {
            if d > s.range {
                0.0
            } else {
                s.damage / s.cooldown.max(0.05) * 0.9
            }
        }
        WK::Mortar => {
            if d > 220.0 || d < 12.0 {
                0.0
            } else {
                (s.damage * 0.4 + s.splash * 0.5) / s.cooldown.max(0.1) * 0.6
            }
        }
    }
}

impl Bot {
    pub fn new(hello_cfg: Config, world: Arc<World>, nav: Nav, seed: u64, id: i32, p: Params) -> Bot {
        Bot {
            cfg: hello_cfg,
            world,
            nav,
            p,
            rng: Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1),
            id,
            tracks: Vec::new(),
            path: Vec::new(),
            path_goal: (1e9, 1e9),
            path_t: -100.0,
            strafe: 1.0,
            strafe_until: 0.0,
            jink: 1.0,
            jink_until: 0.0,
            stuck_t: 0.0,
            unstick_until: -1.0,
            last_dmg_t: -100.0,
            npc_hurt: Vec::new(),
            upg_taken: Vec::new(),
            cover_check: (-100.0, true),
            unseen: Vec::new(),
            last_offer: -1,
            last_shot_t: -100.0,
            dmg_window: Vec::new(),
            mode: Mode::Hold,
            hold: None,
            hold_key: -1.0,
            blacklist: Vec::new(),
            loot_track: None,
            target_id: None,
            mem_pk: Vec::new(),
            mem_cube: Vec::new(),
            mem_crate: Vec::new(),
            last_fight: false,
            decide_lock: 0.0,
            last_t: 0.0,
            scan_yaw: 0.0,
            last_sound: None,
            cover: None,
            label: String::new(),
            debug: false,
        }
    }

    fn scale(&self, me: &SelfObs) -> f64 {
        (me.stats.radius / self.cfg.mech.radius.max(0.1)).max(1.0)
    }

    fn zone_at(&self, z: &Zone, t: f64) -> ([f64; 2], f64) {
        if t <= z.shrink_start || z.shrink_end <= z.shrink_start {
            if t >= z.shrink_end && z.shrink_end > 0.0 {
                return (z.next_center, z.next_radius);
            }
            return (z.center, z.radius);
        }
        let f = ((t - z.shrink_start) / (z.shrink_end - z.shrink_start)).clamp(0.0, 1.0);
        (
            [z.center[0] + (z.next_center[0] - z.center[0]) * f, z.center[1] + (z.next_center[1] - z.center[1]) * f],
            z.radius + (z.next_radius - z.radius) * f,
        )
    }

    fn upd_tracks(&mut self, o: &Obs) {
        for m in &o.mechs {
            if !m.is_mech || m.id == o.me.id {
                continue;
            }
            if let Some(tr) = self.tracks.iter_mut().find(|t| t.id == m.id) {
                tr.pos = m.pos;
                tr.vel = m.vel;
                tr.aim = m.aim;
                if m.hp >= 0.0 {
                    tr.hp = m.hp;
                    tr.max_hp = m.max_hp;
                }
                tr.cubes = m.cubes;
                tr.weapons = m.weapons.clone();
                tr.charging = m.charging;
                tr.seen_t = o.t;
            } else {
                self.tracks.push(Track {
                    id: m.id,
                    pos: m.pos,
                    vel: m.vel,
                    aim: m.aim,
                    hp: if m.hp >= 0.0 { m.hp } else { 1400.0 },
                    max_hp: if m.max_hp > 0.0 { m.max_hp } else { 1400.0 },
                    cubes: m.cubes,
                    weapons: m.weapons.clone(),
                    charging: m.charging,
                    seen_t: o.t,
                    first_t: o.t,
                    dmg_to_me: 0.0,
                    last_hit_me_t: -100.0,
                });
            }
        }
        for e in &o.events {
            if e.typ == "damaged" && e.weapon == "npc" {
                self.npc_hurt.push((e.by.unwrap_or(-1), e.bearing.unwrap_or(0.0), o.t));
            }
            if e.typ == "damaged" {
                self.last_dmg_t = o.t;
                self.dmg_window.push((o.t, e.amount));
                if e.weapon != "zone" && e.by.is_none() {
                    if let Some(b) = e.bearing {
                        self.last_sound = Some((b, 60.0, o.t));
                    }
                }
                if let Some(by) = e.by {
                    if e.weapon != "npc" && !o.mechs.iter().any(|m| m.id == by) {
                        self.unseen.push((by, e.bearing.unwrap_or(0.0), o.t, e.rear, e.amount));
                    }
                    if let Some(tr) = self.tracks.iter_mut().find(|t| t.id == by) {
                        tr.dmg_to_me += e.amount;
                        tr.last_hit_me_t = o.t;
                    }
                    if let Some(b) = e.bearing {
                        self.last_sound = Some((b, 80.0, o.t));
                    }
                }
            }
            if e.typ == "kill" {
                let tid = e.target as i32;
                self.tracks.retain(|t| t.id != tid);
            }
            if e.typ == "take_failed" {
                self.blacklist.push((e.id, o.t + 8.0));
            }
            if e.typ == "hit" {
                self.last_shot_t = o.t;
            }
        }
        // death sounds: can't attribute; drop very stale tracks
        self.tracks.retain(|t| o.t - t.seen_t < 25.0);
        self.dmg_window.retain(|d| o.t - d.0 < 4.0);
        self.npc_hurt.retain(|d| o.t - d.2 < 4.0);
        self.unseen.retain(|d| o.t - d.2 < 3.0);
        self.blacklist.retain(|b| b.1 > o.t);
        for s in &o.sounds {
            if s.kind == "gun" || s.kind == "laser" || s.kind == "lightning" || s.kind == "mortar" || s.kind == "step" || s.kind == "boost" || s.kind == "laser_charge" {
                let d = match s.dist {
                    0 => 35.0,
                    1 => 110.0,
                    _ => 220.0,
                };
                let better = match self.last_sound {
                    Some((_, dd, tt)) => o.t - tt > 1.5 || d <= dd,
                    None => true,
                };
                if better {
                    self.last_sound = Some((s.bearing, d, o.t));
                }
            }
        }
    }

    fn upd_memory(&mut self, o: &Obs) {
        let me = &o.me;
        let eye = [me.pos[0], me.pos[1], me.pos[2] + me.stats.eye_height.max(8.0)];
        for p in &o.pickups {
            match self.mem_pk.iter_mut().find(|x| x.0 == p.id) {
                Some(x) => x.2 = p.pos,
                None => self.mem_pk.push((p.id, p.kind, p.pos)),
            }
        }
        for c in &o.cubes {
            if !self.mem_cube.iter().any(|x| x.0 == c.id) {
                self.mem_cube.push((c.id, c.pos));
            }
        }
        for c in &o.crates {
            if !self.mem_crate.iter().any(|x| x.0 == c.id) {
                self.mem_crate.push((c.id, c.pos));
            }
        }
        let w = &self.world;
        let gone = |id: i64, pos: &V3, visible: bool| -> bool {
            if visible {
                return false;
            }
            let d = d2(pos, &me.pos);
            d < 38.0 && w.los(eye, [pos[0], pos[1], pos[2] + 0.6])
        };
        let pk_ids: Vec<i64> = o.pickups.iter().map(|p| p.id).collect();
        self.mem_pk.retain(|x| !gone(x.0, &x.2, pk_ids.contains(&x.0)));
        let cu_ids: Vec<i64> = o.cubes.iter().map(|p| p.id).collect();
        self.mem_cube.retain(|x| !gone(x.0, &x.1, cu_ids.contains(&x.0)));
        let cr_ids: Vec<i64> = o.crates.iter().map(|p| p.id).collect();
        self.mem_crate.retain(|x| !gone(x.0, &x.1, cr_ids.contains(&x.0)));
        for e in &o.events {
            if e.typ == "took" {
                let id = e.id;
                self.mem_pk.retain(|x| x.0 != id);
            }
        }
    }

    fn choose_upgrade(&mut self, of: &Offer) -> usize {
        if self.p.upg_v2 {
            // rough % gain in effective HP or overall DPS per stack
            let val = |id: &str, n: usize| -> f64 {
                let v = match id {
                    "plating" => 15.0,
                    "gun_mag" => [12.0, 6.0][n.min(1)],
                    "nanites" => 10.0,
                    "armor" => 9.0 + n as f64 * 0.8,
                    "laser_damage" => 8.0,
                    "gun_damage" => 7.0,
                    "lightning_efficiency" => 6.0,
                    "lightning_range" => 6.0,
                    "laser_charge" => 5.0,
                    "gun_velocity" => 5.0,
                    "gun_rate" => 4.0,
                    "lightning_chain" => 4.0,
                    "servos" => 3.0,
                    "sensors" => 2.5,
                    "gyros" => 2.0,
                    "reactor" => 2.0,
                    "mortar_reload" => 2.0,
                    "capacitor" => 1.5,
                    "jets" => 1.5,
                    "mortar_splash" => 1.5,
                    "mortar_velocity" => 1.0,
                    "laser_pierce" => 1.0,
                    _ => 0.5,
                };
                v
            };
            let mut best = (f64::MIN, 0usize);
            for (i, c) in of.choices.iter().enumerate() {
                let n = self.upg_taken.iter().filter(|x| *x == c).count();
                let v = val(c.as_str(), n);
                if v > best.0 {
                    best = (v, i);
                }
            }
            if self.last_offer != of.id {
                self.last_offer = of.id;
                if let Some(c) = of.choices.get(best.1) {
                    self.upg_taken.push(c.clone());
                }
            }
            return best.1;
        }
        let mut best = (usize::MAX, 0usize);
        for (i, c) in of.choices.iter().enumerate() {
            let r = self.p.upgrade_order.iter().position(|u| u == c).unwrap_or(100);
            if r < best.0 {
                best = (r, i);
            }
        }
        best.1
    }

    /// Estimated DPS a mech with these weapons deals at distance d.
    fn enemy_dps(&self, weapons: &[WK], d: f64, cubes: f64) -> f64 {
        let mut s = 0.0;
        let mut any = false;
        for w in weapons {
            s += wdps(*w, d, &self.cfg, None);
            any = true;
        }
        if !any {
            s += wdps(WK::Gun, d, &self.cfg, None);
        }
        s * (1.0 + self.cfg.damage_per_cube * cubes.min(self.cfg.stat_cap))
    }

    fn my_dps(&self, me: &SelfObs, d: f64) -> f64 {
        let mut s = 0.0;
        for w in me.weapons.iter().flatten() {
            s += wdps(w.kind, d, &self.cfg, Some(&w.stats));
        }
        s * (1.0 + self.cfg.damage_per_cube * me.cubes.min(self.cfg.stat_cap))
    }

    /// Can a mech at `from` (feet) see a point `to` (world)?
    fn sees(&self, from: V3, from_scale: f64, to: V3) -> bool {
        let eye = [from[0], from[1], from[2] + 8.0 * from_scale];
        self.world.los(eye, to)
    }

    fn walk_dir_ok(&self, me: &SelfObs, dx: f64, dy: f64, dist: f64) -> bool {
        let (nx, ny) = norm2(dx, dy);
        let (ax, ay) = (me.pos[0], me.pos[1]);
        let (bx, by) = (ax + nx * dist, ay + ny * dist);
        match (self.nav.nearest_stand(ax, ay, Some(me.pos[2]), 2), self.nav.idx(bx, by)) {
            (Some(_), Some(kb)) => self.nav.stand[kb] && self.nav.walkable_line(ax, ay, bx, by),
            _ => false,
        }
    }

    /// Move toward goal via A* path; returns move vector.
    fn nav_to(&mut self, me: &SelfObs, t: f64, gx: f64, gy: f64) -> [f64; 2] {
        let (x, y) = (me.pos[0], me.pos[1]);
        let dg = ((gx - x).powi(2) + (gy - y).powi(2)).sqrt();
        if dg < 1.5 {
            return [0.0, 0.0];
        }
        let replan = (gx - self.path_goal.0).abs() + (gy - self.path_goal.1).abs() > 6.0
            || t - self.path_t > 2.5
            || self.path.is_empty();
        if replan {
            if let Some(p) = self.nav.path(x, y, me.pos[2], gx, gy, 60000, &|_| 0.0) {
                self.path = p;
            } else {
                self.path.clear();
            }
            self.path_goal = (gx, gy);
            self.path_t = t;
        }
        // drop waypoints we've reached
        while self.path.len() > 1 {
            let (px, py) = self.path[0];
            if (px - x).powi(2) + (py - y).powi(2) < 9.0 {
                self.path.remove(0);
            } else {
                break;
            }
        }
        if self.path.is_empty() {
            let (nx, ny) = norm2(gx - x, gy - y);
            return [nx, ny];
        }
        // farthest visible waypoint within 40 m
        let mut tgt = self.path[0];
        for i in (0..self.path.len().min(20)).rev() {
            let (px, py) = self.path[i];
            if (px - x).powi(2) + (py - y).powi(2) > 1600.0 {
                continue;
            }
            if self.nav.walkable_line(x, y, px, py) {
                tgt = (px, py);
                break;
            }
        }
        let (nx, ny) = norm2(tgt.0 - x, tgt.1 - y);
        [nx, ny]
    }

    fn zone_goal(&mut self, o: &Obs) -> (f64, f64) {
        let z = &o.zone;
        let me = &o.me;
        let key = z.next_center[0] * 7.0 + z.next_center[1] * 3.0 + z.next_radius;
        if self.hold.is_none() || (key - self.hold_key).abs() > 1e-6 {
            self.hold_key = key;
            // hold point: from next center toward me, at hold_frac * next radius
            let dx = me.pos[0] - z.next_center[0];
            let dy = me.pos[1] - z.next_center[1];
            let d = (dx * dx + dy * dy).sqrt();
            let r = z.next_radius * self.p.hold_frac;
            let (mut hx, mut hy) = if d <= r {
                (me.pos[0], me.pos[1])
            } else {
                (z.next_center[0] + dx / d * r, z.next_center[1] + dy / d * r)
            };
            // make sure it is standable ground
            if let Some(k) = self.nav.nearest_stand(hx, hy, None, 12) {
                let c = self.nav.center(k);
                hx = c.0;
                hy = c.1;
            }
            self.hold = Some((hx, hy));
        }
        self.hold.unwrap()
    }

    /// Find a nearby standable spot hidden from all given threat positions.
    fn find_cover(&self, me: &SelfObs, threats: &[(V3, f64)], zc: [f64; 2], zr: f64) -> Option<(f64, f64)> {
        let sc = self.scale(me);
        let mut best: Option<(f64, f64, f64)> = None;
        for ring in [8.0, 15.0, 24.0, 34.0, 46.0] {
            for k in 0..20 {
                let a = k as f64 / 20.0 * 2.0 * PI;
                let cx = me.pos[0] + ring * a.cos();
                let cy = me.pos[1] + ring * a.sin();
                let kk = match self.nav.idx(cx, cy) {
                    Some(kk) => kk,
                    None => continue,
                };
                if !self.nav.stand[kk] {
                    continue;
                }
                let h = self.nav.hgt[kk] as f64;
                if (h - me.pos[2]).abs() > 6.0 {
                    continue;
                }
                if ((cx - zc[0]).powi(2) + (cy - zc[1]).powi(2)).sqrt() > zr - 5.0 {
                    continue;
                }
                let mut exposed = 0.0;
                for (tp, tsc) in threats {
                    let chest = [cx, cy, h + 4.5 * sc];
                    let head = [cx, cy, h + 8.5 * sc];
                    if self.sees(*tp, *tsc, chest) || self.sees(*tp, *tsc, head) {
                        exposed += 1.0;
                    }
                }
                let mut score = exposed * 100.0 + ring * 0.6;
                // prefer away from threats
                for (tp, _) in threats {
                    let d = ((tp[0] - cx).powi(2) + (tp[1] - cy).powi(2)).sqrt();
                    score -= d.min(150.0) * 0.05;
                }
                if exposed > 0.0 {
                    continue;
                }
                if !self.nav.walkable_line(me.pos[0], me.pos[1], cx, cy) && ring > 10.0 {
                    score += 15.0;
                }
                if best.map_or(true, |b| score < b.2) {
                    best = Some((cx, cy, score));
                }
            }
            if best.is_some() && ring >= 24.0 {
                break;
            }
        }
        best.map(|b| (b.0, b.1))
    }

    /// True when retreating is pointless: no hidden spot nearby and a lone attacker we can shoot back at.
    fn no_cover_stand(&mut self, me: &SelfObs, t: f64, threats: &[(V3, f64)], zc: [f64; 2], zr: f64, lone_target: bool) -> bool {
        if !self.p.stand_no_cover || !lone_target {
            return false;
        }
        if t - self.cover_check.0 > 1.0 {
            self.cover_check = (t, self.find_cover(me, threats, zc, zr).is_some());
        }
        !self.cover_check.1
    }

    pub fn act(&mut self, o: &Obs) -> Action {
        let t = o.t;
        let me = &o.me;
        let sc = self.scale(me);
        self.id = me.id;
        self.upd_tracks(o);
        self.upd_memory(o);
        let mut a = Action::default();
        if let Some(of) = &o.offer {
            a.pick = Some(self.choose_upgrade(of));
        }
        let my_eye = [me.pos[0], me.pos[1], me.pos[2] + me.stats.eye_height.max(8.0 * sc)];
        let my_muz = [me.pos[0], me.pos[1], me.pos[2] + me.stats.muzzle_height.max(7.0 * sc)];
        let hp_frac = me.hp / me.max_hp.max(1.0);

        // ---------------- threat assessment
        struct En {
            id: i32,
            pos: V3,
            vel: V3,
            hp: f64,
            max_hp: f64,
            d: f64,
            los: bool,
            sees_me: bool,
            dps: f64,
            fresh: bool,
            facing: bool,
            weapons: Vec<WK>,
            charging: bool,
            aim: [f64; 2],
            scale: f64,
        }
        let mut ens: Vec<En> = Vec::new();
        for tr in &self.tracks {
            let age = t - tr.seen_t;
            if age > 6.0 {
                continue;
            }
            let tsc = 1.0 + self.cfg.size_per_cube * tr.cubes;
            let pos = add(tr.pos, mul(tr.vel, age.min(1.0)));
            let d = d3(&pos, &me.pos);
            let tc = [pos[0], pos[1], pos[2] + 4.5 * tsc];
            let los = self.world.los(my_muz, tc) || self.world.los(my_eye, [pos[0], pos[1], pos[2] + 8.0 * tsc]);
            let their_eye = [pos[0], pos[1], pos[2] + 8.0 * tsc];
            let sees_me_los = self.world.los(their_eye, [me.pos[0], me.pos[1], me.pos[2] + 4.5 * sc])
                || self.world.los(their_eye, [me.pos[0], me.pos[1], me.pos[2] + 8.5 * sc]);
            let yaw_to_me = yaw_to(&pos, &me.pos);
            let facing = wrap(yaw_to_me - tr.aim[0]).abs() < (self.cfg.fov_deg.to_radians() / 2.0 + 0.1);
            let sees_me = sees_me_los && (d < self.cfg.near_range + 5.0 || (facing && d < self.cfg.vis_range + 10.0)) || t - tr.last_hit_me_t < 2.0;
            let dps = self.enemy_dps(&tr.weapons, d, tr.cubes);
            ens.push(En {
                id: tr.id,
                pos,
                vel: tr.vel,
                hp: tr.hp,
                max_hp: tr.max_hp.max(1.0),
                d,
                los,
                sees_me,
                dps,
                fresh: age < 0.25,
                facing,
                weapons: tr.weapons.clone(),
                charging: tr.charging,
                aim: tr.aim,
                scale: tsc,
            });
        }

        // ---------------- hostile drones: shooting us, or close enough to start
        let mut hostile_npc: Option<(V3, V3, f64)> = None; // pos, vel, dist
        for n in o.mechs.iter().filter(|m| !m.is_mech) {
            let d = d3(&n.pos, &me.pos);
            let by_id = self.npc_hurt.iter().any(|h| h.0 == n.id);
            let by_bearing = self.npc_hurt.iter().any(|h| h.0 < 0 && wrap(h.1 - yaw_to(&me.pos, &n.pos)).abs() < 0.3) && d < 100.0;
            let close = d < self.cfg.npc_aggro + 3.0;
            if !self.p.npc_fix || !(by_id || by_bearing || close) {
                continue;
            }
            let cp = [n.pos[0], n.pos[1], n.pos[2] + self.cfg.npc_height * 0.5];
            if !self.world.los(my_muz, cp) {
                continue;
            }
            if hostile_npc.map_or(true, |h| d < h.2) {
                hostile_npc = Some((cp, n.vel, d));
            }
        }

        // ---------------- target selection
        let mut target: Option<usize> = None;
        let mut best_score = f64::MAX;
        let max_range = me
            .weapons
            .iter()
            .flatten()
            .map(|w| match w.kind {
                WK::Gun => w.stats.range * 0.95,
                WK::Laser => w.stats.range,
                WK::Lightning => w.stats.range,
                WK::Mortar => 220.0,
            })
            .fold(0.0, f64::max);
        for (i, e) in ens.iter().enumerate() {
            if !e.los || !e.fresh {
                continue;
            }
            if e.d > max_range {
                continue;
            }
            // expected seconds to kill + slew time, with stickiness
            let dps = self.my_dps(me, e.d).max(20.0);
            let slew = wrap(yaw_to(&me.pos, &e.pos) - me.aim[0]).abs() / me.stats.turn_rate.max(1.0);
            let mut s = e.hp / dps + slew * 2.0 + e.d / 400.0;
            if e.sees_me {
                s -= 0.8;
            }
            if !e.facing {
                s -= 0.3;
            }
            if Some(e.id) == self.target_id {
                s -= 1.2;
            }
            if s < best_score {
                best_score = s;
                target = Some(i);
            }
        }

        self.target_id = target.map(|i| ens[i].id);

        // ---------------- zone
        let (zc_now, zr_now) = self.zone_at(&o.zone, t);
        let (zc_soon, zr_soon) = self.zone_at(&o.zone, t + self.p.zone_margin_s);
        let d_center_soon = d2(&me.pos, &zc_soon);
        let outside_soon = d_center_soon > zr_soon - 6.0;
        let d_center_now = d2(&me.pos, &zc_now);
        let hold = self.zone_goal(o);
        // time needed to reach next circle
        let dn = d2(&me.pos, &o.zone.next_center);
        let need = ((dn - o.zone.next_radius * 0.8).max(0.0)) / (me.stats.walk_speed * 0.8);
        let late = t + need + self.p.zone_margin_s > o.zone.shrink_end && dn > o.zone.next_radius * 0.85;

        // ---------------- engagement decision
        let engaged: Vec<usize> = ens.iter().enumerate().filter(|(_, e)| e.sees_me && e.d < 260.0).map(|(i, _)| i).collect();
        let npc_dps = if self.p.npc_fix && !self.npc_hurt.is_empty() { self.cfg.npc_damage / self.cfg.npc_cooldown.max(0.1) } else { 0.0 };
        let unseen_dps: f64 = if self.p.face_unseen { self.unseen.iter().map(|u| u.4).sum::<f64>() / 3.0 } else { 0.0 };
        let incoming: f64 = (engaged.iter().map(|&i| ens[i].dps).sum::<f64>() + npc_dps + unseen_dps).max(1.0);
        let my_ttl = me.hp / incoming;
        let mut fight = false;
        if let Some(ti) = target {
            let e = &ens[ti];
            let my_dps = self.my_dps(me, e.d).max(1.0);
            let ttk = e.hp / my_dps;
            let advantage = ttk < my_ttl * self.p.fight_ratio;
            let their_ttk = me.hp / e.dps.max(1.0);
            let initiate = !self.p.passive
                || e.sees_me
                || e.hp < 0.55 * e.max_hp
                || (!e.facing && e.d < 140.0)
                || ttk * 2.0 < their_ttk;
            fight = advantage && initiate;
            if engaged.is_empty() && !self.p.passive {
                fight = true;
            }
            if hp_frac < self.p.retreat_hp && !engaged.is_empty() && ttk > 1.5 {
                fight = false;
            }
            // cornered at short range: fight
            if !fight && e.sees_me && e.d < 22.0 {
                fight = true;
            }
            // finish low targets
            if e.hp < 260.0 && e.d < 160.0 && hp_frac > 0.2 {
                fight = true;
            }
            // hysteresis
            if t < self.decide_lock {
                if self.last_fight && hp_frac > self.p.retreat_hp - 0.05 {
                    fight = true;
                }
                if !self.last_fight && e.hp > 260.0 {
                    fight = false;
                }
            }
            if fight != self.last_fight {
                self.last_fight = fight;
                self.decide_lock = t + if fight { 1.5 } else { 2.0 };
            }
        }
        let recent_dmg: f64 = self.dmg_window.iter().map(|d| d.1).sum();

        // no hidden spot to run to from a lone attacker: stand and fight
        let stand = !engaged.is_empty()
            && (!fight || hp_frac < self.p.retreat_hp)
            && self.no_cover_stand(
                me,
                t,
                &engaged.iter().map(|&i| (ens[i].pos, ens[i].scale)).collect::<Vec<_>>(),
                zc_soon,
                zr_soon,
                engaged.len() == 1 && target.map_or(false, |ti| engaged.contains(&ti)),
            );
        if stand {
            fight = true;
        }

        // ---------------- movement
        let mut mv = [0.0, 0.0];
        let mut boost = false;
        let mut jump = false;
        let mut mode = Mode::Hold;
        let in_zone_ok = !outside_soon && !late;
        if !me.in_zone || (outside_soon && d_center_now > zr_now * 0.5) || late {
            mode = Mode::Zone;
            let gx = if me.in_zone { hold.0 } else { zc_soon[0] };
            let gy = if me.in_zone { hold.1 } else { zc_soon[1] };
            let (gx, gy) = if !me.in_zone {
                // closest point inside the soon-circle
                let dx = me.pos[0] - zc_soon[0];
                let dy = me.pos[1] - zc_soon[1];
                let d = (dx * dx + dy * dy).sqrt().max(1e-6);
                let r = (zr_soon * 0.7).max(0.0);
                if d > r {
                    (zc_soon[0] + dx / d * r, zc_soon[1] + dy / d * r)
                } else {
                    (gx, gy)
                }
            } else {
                (gx, gy)
            };
            mv = self.nav_to(me, t, gx, gy);
            boost = !me.in_zone || late;
        } else if !engaged.is_empty() && (!fight || hp_frac < self.p.retreat_hp) && !stand {
            mode = Mode::Retreat;
            let threats: Vec<(V3, f64)> = engaged.iter().map(|&i| (ens[i].pos, ens[i].scale)).collect();
            let need_new = match self.cover {
                Some((cx, cy, ct)) => {
                    t - ct > 3.0 || threats.iter().any(|(tp, ts)| self.sees(*tp, *ts, [cx, cy, me.pos[2] + 4.5 * sc]))
                }
                None => true,
            };
            if need_new {
                self.cover = self.find_cover(me, &threats, zc_soon, zr_soon).map(|(x, y)| (x, y, t));
            }
            match self.cover {
                Some((cx, cy, _)) => {
                    mv = self.nav_to(me, t, cx, cy);
                    boost = me.energy > 25.0;
                }
                None => {
                    // run away from the threat centroid, toward the zone
                    let mut ax = 0.0;
                    let mut ay = 0.0;
                    for (tp, _) in &threats {
                        let (nx, ny) = norm2(me.pos[0] - tp[0], me.pos[1] - tp[1]);
                        ax += nx;
                        ay += ny;
                    }
                    let (zx, zy) = norm2(hold.0 - me.pos[0], hold.1 - me.pos[1]);
                    let (nx, ny) = norm2(ax + zx * 0.5, ay + zy * 0.5);
                    let gx = me.pos[0] + nx * 40.0;
                    let gy = me.pos[1] + ny * 40.0;
                    mv = self.nav_to(me, t, gx, gy);
                    boost = me.energy > 25.0;
                }
            }
        } else if let (Some(ti), true) = (target, fight) {
            mode = Mode::Fight;
            let e = &ens[ti];
            // preferred range
            let has = |k: WK| me.weapons.iter().flatten().any(|w| w.kind == k);
            let they_light = e.weapons.contains(&WK::Lightning);
            let they_laser = e.weapons.contains(&WK::Laser);
            let pref = if !self.p.new_pref {
                if has(WK::Lightning) && (!they_light || hp_frac > 0.5) {
                    24.0
                } else if they_light {
                    75.0
                } else if has(WK::Laser) {
                    110.0
                } else if has(WK::Mortar) {
                    90.0
                } else {
                    65.0
                }
            } else if has(WK::Lightning) && !they_light && hp_frac > 0.35 {
                26.0
            } else if has(WK::Lightning) && they_light {
                if has(WK::Laser) && !they_laser {
                    100.0
                } else if me.hp > e.hp * 1.3 {
                    26.0
                } else {
                    70.0
                }
            } else if they_light {
                72.0
            } else if has(WK::Laser) {
                110.0
            } else if has(WK::Mortar) {
                90.0
            } else {
                65.0
            };
            let (rx, ry) = norm2(e.pos[0] - me.pos[0], e.pos[1] - me.pos[1]);
            let radial = ((e.d - pref) / 30.0).clamp(-1.0, 1.0);
            if t > self.strafe_until {
                self.strafe = if self.rng.f() < 0.5 { 1.0 } else { -1.0 };
                self.strafe_until = t + self.rng.range(0.35, 1.1);
            }
            let (px, py) = (-ry * self.strafe, rx * self.strafe);
            let mut dx = px * 0.9 + rx * radial;
            let mut dy = py * 0.9 + ry * radial;
            if !self.walk_dir_ok(me, dx, dy, 5.0) {
                self.strafe = -self.strafe;
                dx = -px * 0.9 + rx * radial;
                dy = -py * 0.9 + ry * radial;
                if !self.walk_dir_ok(me, dx, dy, 5.0) {
                    // fall back to path toward target vicinity
                    let m = self.nav_to(me, t, e.pos[0], e.pos[1]);
                    dx = m[0];
                    dy = m[1];
                }
            }
            // keep away from a drone that is shooting us (it gives up beyond ~90 m / its leash)
            if let Some((np, _, nd)) = hostile_npc {
                if self.p.npc_fix && nd < 70.0 && !self.npc_hurt.is_empty() {
                    let (ax, ay) = norm2(me.pos[0] - np[0], me.pos[1] - np[1]);
                    dx += ax * 0.7;
                    dy += ay * 0.7;
                }
            }
            // stay in zone: blend toward hold if near edge
            if d_center_soon > zr_soon * 0.85 {
                let (zx, zy) = norm2(zc_soon[0] - me.pos[0], zc_soon[1] - me.pos[1]);
                dx += zx * 0.8;
                dy += zy * 0.8;
            }
            let (nx, ny) = norm2(dx, dy);
            mv = [nx, ny];
            boost = me.energy > 60.0 && e.d > pref + 40.0;
            // reloading with nothing else to shoot: duck behind cover
            let gun_reload = me.weapons.iter().flatten().find(|w| w.kind == WK::Gun).map_or(0.0, |w| w.reload);
            let other_ready = me.weapons.iter().flatten().any(|w| w.kind != WK::Gun && w.cooldown < 0.4 && (w.kind != WK::Lightning || e.d < w.stats.range));
            if self.p.reload_cover && gun_reload > 0.9 && !other_ready && e.hp > 250.0 && e.sees_me {
                let threats: Vec<(V3, f64)> = engaged.iter().map(|&i| (ens[i].pos, ens[i].scale)).collect();
                if let Some((cx, cy)) = self.find_cover(me, &threats, zc_soon, zr_soon) {
                    if ((cx - me.pos[0]).powi(2) + (cy - me.pos[1]).powi(2)).sqrt() < 30.0 {
                        mv = self.nav_to(me, t, cx, cy);
                        boost = me.energy > 30.0;
                    }
                }
            }
            if e.d < 60.0 && me.grounded && self.rng.f() < 0.08 && me.energy > 40.0 {
                jump = true;
            }
        } else {
            // loot or hold
            let mut goal: Option<(f64, f64, f64, i64)> = None; // x,y,score,id
            let want = |k: WK, me: &SelfObs, p: &Params| -> f64 {
                if me.weapons.iter().flatten().any(|w| w.kind == k) {
                    return 0.0;
                }
                let worst = match &me.weapons[1] {
                    None => 0.0,
                    Some(w) => p.wpref[w.kind as usize],
                };
                let v = p.wpref[k as usize] - worst;
                if v > 0.05 {
                    v
                } else {
                    0.0
                }
            };
            let safe = engaged.is_empty() && recent_dmg < 1.0;
            if safe || (engaged.is_empty() && hp_frac > 0.6) {
                let mem_pk: Vec<Pickup> = self.mem_pk.iter().map(|x| Pickup { id: x.0, kind: x.1, pos: x.2 }).collect();
                for pk in &mem_pk {
                    if self.blacklist.iter().any(|b| b.0 == pk.id) {
                        continue;
                    }
                    let v = want(pk.kind, me, &self.p);
                    if v <= 0.0 {
                        continue;
                    }
                    let d = d3(&pk.pos, &me.pos);
                    if d > self.p.loot_range || (pk.pos[2] - me.pos[2]).abs() > 3.5 && d < 4.0 {
                        continue;
                    }
                    // reachable?
                    if let Some(k) = self.nav.nearest_stand(pk.pos[0], pk.pos[1], Some(pk.pos[2]), 2) {
                        if (self.nav.hgt[k] as f64 - pk.pos[2]).abs() > 3.0 {
                            continue;
                        }
                    } else {
                        continue;
                    }
                    if d2(&pk.pos, &zc_soon) > zr_soon - 10.0 {
                        continue;
                    }
                    let s = v * 3.0 / (d + 15.0);
                    if goal.map_or(true, |g| s > g.2) {
                        goal = Some((pk.pos[0], pk.pos[1], s, pk.id));
                    }
                }
                let mem_cube: Vec<Item> = self.mem_cube.iter().map(|x| Item { id: x.0, pos: x.1, hp: 0.0 }).collect();
                for c in &mem_cube {
                    if self.blacklist.iter().any(|b| b.0 == c.id) {
                        continue;
                    }
                    let d = d3(&c.pos, &me.pos);
                    if d > self.p.loot_range {
                        continue;
                    }
                    if let Some(k) = self.nav.nearest_stand(c.pos[0], c.pos[1], Some(c.pos[2]), 2) {
                        if (self.nav.hgt[k] as f64 - c.pos[2]).abs() > 3.0 {
                            continue;
                        }
                    } else {
                        continue;
                    }
                    if d2(&c.pos, &zc_soon) > zr_soon - 10.0 {
                        continue;
                    }
                    let s = 2.0 / (d + 15.0);
                    if goal.map_or(true, |g| s > g.2) {
                        goal = Some((c.pos[0], c.pos[1], s, c.id));
                    }
                }
            }
            if safe || (engaged.is_empty() && hp_frac > 0.6) {
                let mem_cr: Vec<(i64, V3)> = self.mem_crate.clone();
                for (cid, cp) in &mem_cr {
                    if self.blacklist.iter().any(|b| b.0 == *cid) {
                        continue;
                    }
                    let d = d3(cp, &me.pos);
                    if d < 30.0 || d > 110.0 {
                        continue;
                    }
                    if (cp[2] - me.pos[2]).abs() > 6.0 {
                        continue; // roof crate
                    }
                    if d2(cp, &zc_soon) > zr_soon - 10.0 {
                        continue;
                    }
                    let s = 1.2 / (d + 25.0);
                    if goal.map_or(true, |g| s > g.2) {
                        goal = Some((cp[0], cp[1], s, *cid));
                    }
                }
            }
            // supply drops: full heal + 3 cubes if we can hold the ring alone
            if engaged.is_empty() {
                for dr in &o.drops {
                    let d = d2(&dr.pos, &me.pos);
                    let eta = d / me.stats.walk_speed;
                    if d > self.p.drop_range || dr.lands_in > eta + 20.0 {
                        continue;
                    }
                    if let Some(h) = dr.holder {
                        if h != me.id {
                            continue;
                        }
                    }
                    let contested = ens.iter().any(|e| d2(&e.pos, &dr.pos) < 70.0 && t - 0.0 >= 0.0);
                    if contested {
                        continue;
                    }
                    if d2(&dr.pos, &zc_soon) > zr_soon - 5.0 {
                        continue;
                    }
                    let s = 6.0 / (d + 20.0);
                    if goal.map_or(true, |g| s > g.2) {
                        goal = Some((dr.pos[0], dr.pos[1], s, dr.id));
                    }
                }
            }
            if let Some((gx, gy, _, gid)) = goal {
                mode = Mode::Loot;
                let is_drop = gid >= 20000;
                // progress tracking: blacklist goals we can't get closer to
                let dnow = ((gx - me.pos[0]).powi(2) + (gy - me.pos[1]).powi(2)).sqrt();
                match self.loot_track {
                    Some((id, _, best)) if id == gid => {
                        if dnow < best - 2.0 {
                            self.loot_track = Some((gid, t, dnow));
                        }
                    }
                    _ => self.loot_track = Some((gid, t, dnow)),
                }
                if let Some((_, since, _)) = self.loot_track {
                    if t - since > 5.0 && !(is_drop && dnow < 6.0) {
                        self.blacklist.push((gid, t + 60.0));
                        self.loot_track = None;
                    }
                }
                mv = self.nav_to(me, t, gx, gy);
            } else {
                // hold position near hold point, drift slowly
                let dh = ((hold.0 - me.pos[0]).powi(2) + (hold.1 - me.pos[1]).powi(2)).sqrt();
                if dh > 6.0 {
                    mv = self.nav_to(me, t, hold.0, hold.1);
                    boost = dh > 150.0 && me.energy > 70.0 && in_zone_ok;
                    // no hurry: walk below the step-sound speed
                    let dz_c = d2(&me.pos, &o.zone.next_center);
                    let slack = (o.zone.shrink_end - t) - (dz_c - o.zone.next_radius * 0.6).max(0.0) / (me.stats.walk_speed * 0.4);
                    if self.p.silent && slack > 20.0 && dh < 200.0 {
                        let f = (self.cfg.step_speed * 0.9) / me.stats.walk_speed;
                        mv = [mv[0] * f, mv[1] * f];
                        boost = false;
                    }
                }
            }
        }

        // evasive jinking whenever someone is shooting at / can see us (fight mode strafes on its own)
        if mode != Mode::Fight {
            let mut th: Option<(f64, V3)> = None;
            for e in &ens {
                if e.sees_me && e.d < 240.0 && e.fresh && th.map_or(true, |x| e.d < x.0) {
                    th = Some((e.d, e.pos));
                }
            }
            if th.is_none() {
                if let Some((np, _, nd)) = hostile_npc {
                    if !self.npc_hurt.is_empty() {
                        th = Some((nd, np));
                    }
                }
            }
            if th.is_none() && t - self.last_dmg_t < 1.0 {
                if let Some((b, _, _)) = self.last_sound {
                    th = Some((80.0, [me.pos[0] + b.cos() * 80.0, me.pos[1] + b.sin() * 80.0, me.pos[2]]));
                }
            }
            if let Some((_, tp)) = th {
                if t > self.jink_until {
                    self.jink = if self.rng.f() < 0.5 { 1.0 } else { -1.0 };
                    self.jink_until = t + self.rng.range(0.3, 0.9);
                }
                let (rx, ry) = norm2(tp[0] - me.pos[0], tp[1] - me.pos[1]);
                let (px, py) = (-ry * self.jink, rx * self.jink);
                let base_l = (mv[0] * mv[0] + mv[1] * mv[1]).sqrt();
                let w = if base_l > 0.3 { 0.75 } else { 1.0 };
                let cand = [mv[0] + px * w, mv[1] + py * w];
                if self.walk_dir_ok(me, cand[0], cand[1], 4.0) {
                    let (nx, ny) = norm2(cand[0], cand[1]);
                    mv = [nx, ny];
                } else {
                    let cand2 = [mv[0] - px * w, mv[1] - py * w];
                    if self.walk_dir_ok(me, cand2[0], cand2[1], 4.0) {
                        self.jink = -self.jink;
                        let (nx, ny) = norm2(cand2[0], cand2[1]);
                        mv = [nx, ny];
                    }
                }
            }
        }

        // stuck handling
        let hv = (me.vel[0].powi(2) + me.vel[1].powi(2)).sqrt();
        let want_move = (mv[0].powi(2) + mv[1].powi(2)).sqrt() > 0.3;
        if want_move && hv < 1.5 && me.grounded {
            self.stuck_t += t - self.last_t;
        } else {
            self.stuck_t = 0.0;
        }
        if self.stuck_t > 0.8 {
            self.stuck_t = 0.0;
            self.unstick_until = t + 0.8;
            self.path.clear();
            self.path_t = -100.0;
        }
        if t < self.unstick_until {
            jump = me.grounded;
            boost = me.energy > 10.0;
        }
        // dodge mortar shells
        for p in &o.projectiles {
            if p.kind == 1 {
                // predict landing (z = my feet)
                let g = self.cfg.w(WK::Mortar).gravity;
                let dz = p.pos[2] - me.pos[2];
                let vz = p.vel[2];
                let disc = vz * vz + 2.0 * g * dz;
                if disc > 0.0 {
                    let tl = (vz + disc.sqrt()) / g;
                    let lx = p.pos[0] + p.vel[0] * tl;
                    let ly = p.pos[1] + p.vel[1] * tl;
                    let dd = ((lx - me.pos[0]).powi(2) + (ly - me.pos[1]).powi(2)).sqrt();
                    if dd < 10.0 && tl < 2.5 {
                        let (nx, ny) = norm2(me.pos[0] - lx, me.pos[1] - ly);
                        mv = [nx, ny];
                        boost = me.energy > 10.0;
                    }
                }
            }
        }
        a.mv = mv;
        a.boost = boost;
        a.jump = jump;
        self.mode = mode;

        // ---------------- aiming and firing
        let mut aim = [me.aim[0], 0.0];
        let mut fire = [false, false];
        let laser_slot = me.weapons.iter().position(|w| w.as_ref().map_or(false, |w| w.kind == WK::Laser));
        let laser_charge = laser_slot.and_then(|s| me.weapons[s].as_ref()).map_or(0.0, |w| w.charge);
        if let Some(ti) = target {
            let e = &ens[ti];
            let tc = [e.pos[0], e.pos[1], e.pos[2] + 4.5 * e.scale];
            // primary aim: gun lead if gun, else direct
            let gun = me.weapons.iter().flatten().find(|w| w.kind == WK::Gun);
            let mut aim_pt = tc;
            if let Some(g) = gun {
                if let Some(ti2) = intercept(my_muz, tc, [e.vel[0], e.vel[1], e.vel[2] * 0.3], g.stats.speed) {
                    aim_pt = add(tc, mul([e.vel[0], e.vel[1], e.vel[2] * 0.3], ti2 + 0.05));
                }
            }
            if laser_charge > 0.0 {
                // laser is hitscan: aim at the target's position at release (+ small latency)
                aim_pt = add(tc, mul(e.vel, 0.1));
            }
            let (ay, ap) = angles(sub(aim_pt, my_muz));
            aim = [ay, ap];
            let yaw_err = wrap(ay - me.aim[0]).abs();
            let pitch_err = (ap - me.aim[1]).abs();
            let tol = ((2.2 * e.scale) / e.d.max(1.0)).atan() + 0.01;
            let aligned = yaw_err < tol && pitch_err < tol * 1.5;
            let clear_shot = self.world.los(my_muz, tc);
            for (si, w) in me.weapons.iter().enumerate() {
                let w = match w {
                    Some(w) => w,
                    None => continue,
                };
                match w.kind {
                    WK::Gun => {
                        if aligned && clear_shot && e.d < w.stats.range * 0.97 && w.ammo > 0.0 && w.reload <= 0.0 {
                            fire[si] = true;
                        }
                        if w.ammo <= 0.0 && w.reload <= 0.0 {
                            a.reload = true;
                        }
                    }
                    WK::Laser => {
                        if e.d < w.stats.range * 0.98 && clear_shot {
                            if w.charge <= 0.0 {
                                // start charging when roughly aligned and off cooldown
                                if w.cooldown <= 0.05 && yaw_err < 0.35 {
                                    fire[si] = true;
                                }
                            } else if w.charge + 0.05 < w.stats.charge {
                                fire[si] = true; // keep charging
                            } else {
                                // full: release when aligned
                                let laser_tol = ((1.8 * e.scale) / e.d.max(1.0)).atan() + 0.006;
                                fire[si] = !(yaw_err < laser_tol && pitch_err < laser_tol * 1.5);
                            }
                        } else if w.charge > 0.0 {
                            fire[si] = true; // hold the charge until a shot appears (auto-fires after hold_s)
                        }
                    }
                    WK::Lightning => {
                        let cone = (w.stats.cone_deg * 0.5).to_radians();
                        let yaw_t = yaw_to(&me.pos, &e.pos);
                        if e.d < w.stats.range * 0.97 && wrap(yaw_t - me.aim[0]).abs() < cone * 0.9 && clear_shot && me.energy >= w.stats.energy {
                            fire[si] = true;
                        }
                    }
                    WK::Mortar => {
                        let dxy = d2(&me.pos, &e.pos);
                        if dxy > 18.0 && dxy < 230.0 && w.cooldown <= 0.0 {
                            // ballistic solution with lead
                            let tube = me.pos[2] + 9.5 * sc;
                            let mut tp = [e.pos[0], e.pos[1], e.pos[2] + 1.0];
                            let mut sol = None;
                            for _ in 0..3 {
                                let dx = d2(&me.pos, &tp);
                                if let Some(th) = ballistic_pitch(dx, tp[2] - tube, w.stats.speed, w.stats.gravity, false) {
                                    let tf = dx / (w.stats.speed * th.cos()).max(1.0);
                                    tp = [e.pos[0] + e.vel[0] * tf, e.pos[1] + e.vel[1] * tf, e.pos[2] + 1.0];
                                    sol = Some((yaw_to(&me.pos, &tp), th));
                                } else {
                                    sol = None;
                                    break;
                                }
                            }
                            if let Some((my, mp)) = sol {
                                if si == 0 || gun.is_none() {
                                    aim = [my, mp];
                                }
                                if wrap(my - me.aim[0]).abs() < 0.03 && (mp - me.aim[1]).abs() < 0.03 {
                                    fire[si] = true;
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // no target: shoot crates/npcs if looting & safe, else scan
            let mut shot = false;
            if let Some((cp, vel, d)) = hostile_npc {
                let gun = me.weapons.iter().flatten().find(|w| w.kind == WK::Gun);
                let mut ap = cp;
                if let Some(g) = gun {
                    if let Some(ti) = intercept(my_muz, cp, vel, g.stats.speed) {
                        ap = add(cp, mul(vel, ti));
                    }
                }
                let (ay, apt) = angles(sub(ap, my_muz));
                aim = [ay, apt];
                let tol = (1.4 / d.max(1.0)).atan() + 0.01;
                let yaw_err = wrap(ay - me.aim[0]).abs();
                let aligned = yaw_err < tol && (apt - me.aim[1]).abs() < tol * 1.5;
                for (si, w) in me.weapons.iter().enumerate() {
                    if let Some(w) = w {
                        match w.kind {
                            WK::Gun => {
                                if aligned && w.ammo > 0.0 && w.reload <= 0.0 && d < w.stats.range {
                                    fire[si] = true;
                                }
                                if w.ammo <= 0.0 && w.reload <= 0.0 {
                                    a.reload = true;
                                }
                            }
                            WK::Lightning => {
                                if d < w.stats.range * 0.95 && yaw_err < (w.stats.cone_deg * 0.45).to_radians() && me.energy >= w.stats.energy {
                                    fire[si] = true;
                                }
                            }
                            WK::Laser => {
                                if d < w.stats.range * 0.95 {
                                    if w.charge <= 0.0 {
                                        fire[si] = w.cooldown <= 0.05 && yaw_err < 0.35;
                                    } else if w.charge + 0.05 < w.stats.charge {
                                        fire[si] = true;
                                    } else {
                                        let lt = (1.2 / d.max(1.0)).atan() + 0.006;
                                        fire[si] = !(yaw_err < lt && (apt - me.aim[1]).abs() < lt * 1.5);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                shot = true;
            }
            if !shot && (mode == Mode::Loot || mode == Mode::Hold) {
                let gun = me.weapons.iter().flatten().find(|w| w.kind == WK::Gun);
                let mut best: Option<(V3, V3, f64)> = None;
                for c in &o.crates {
                    let d = d3(&c.pos, &me.pos);
                    if d < 70.0 && engaged.is_empty() && hp_frac > self.p.farm_hp {
                        let cp = [c.pos[0], c.pos[1], c.pos[2] + 1.2];
                        if self.world.los(my_muz, cp) && best.map_or(true, |b| d < b.2) {
                            best = Some((cp, [0.0; 3], d));
                        }
                    }
                }
                if self.p.hunt_npcs && hp_frac > self.p.farm_hp {
                    for n in o.mechs.iter().filter(|m| !m.is_mech) {
                        let d = d3(&n.pos, &me.pos);
                        if d < 90.0 && engaged.is_empty() {
                            let cp = [n.pos[0], n.pos[1], n.pos[2] + 1.75];
                            if self.world.los(my_muz, cp) && best.map_or(true, |b| d < b.2) {
                                best = Some((cp, n.vel, d));
                            }
                        }
                    }
                }
                if let Some((cp, vel, d)) = best {
                    let mut ap = cp;
                    if let Some(g) = gun {
                        if let Some(ti) = intercept(my_muz, cp, vel, g.stats.speed) {
                            ap = add(cp, mul(vel, ti));
                        }
                    }
                    let (ay, apt) = angles(sub(ap, my_muz));
                    aim = [ay, apt];
                    let tol = (1.2 / d.max(1.0)).atan() + 0.01;
                    let aligned = wrap(ay - me.aim[0]).abs() < tol && (apt - me.aim[1]).abs() < tol * 1.5;
                    for (si, w) in me.weapons.iter().enumerate() {
                        if let Some(w) = w {
                            match w.kind {
                                WK::Gun => {
                                    if aligned && w.ammo > 0.0 {
                                        fire[si] = true;
                                    }
                                }
                                WK::Lightning => {
                                    if d < w.stats.range * 0.9 && aligned && me.energy > 50.0 {
                                        fire[si] = true;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    shot = true;
                    // approach to loot the cube / get in good range
                    if mode == Mode::Hold && d > 32.0 {
                        a.mv = self.nav_to(me, t, cp[0], cp[1]);
                    }
                }
            }
            if !shot {
                // face the most relevant threat direction
                let mut look: Option<f64> = None;
                let mut best_d = f64::MAX;
                for e in &ens {
                    if e.d < best_d {
                        best_d = e.d;
                        look = Some(yaw_to(&me.pos, &e.pos));
                    }
                }
                if look.is_none() {
                    if let Some((b, _, st)) = self.last_sound {
                        if t - st < 3.0 {
                            look = Some(b);
                        }
                    }
                }
                let y = match look {
                    Some(y) => y,
                    None => {
                        // scan: sweep around movement direction
                        let base = if want_move { mv[1].atan2(mv[0]) } else { me.aim[0] };
                        self.scan_yaw = base + (t * 1.3).sin() * 1.2;
                        self.scan_yaw
                    }
                };
                aim = [y, 0.0];
                // reload when idle
                for w in me.weapons.iter().flatten() {
                    if w.kind == WK::Gun && w.ammo < w.stats.mag * 0.5 && w.reload <= 0.0 {
                        a.reload = true;
                    }
                }
                // a charged laser with nothing to shoot: keep holding until it fires
                if let Some(s) = laser_slot {
                    if laser_charge > 0.0 {
                        fire[s] = true;
                    }
                }
            }
        }
        // shot in the back by someone we cannot see: face them (front armor, and they become visible)
        if self.p.face_unseen {
            if let Some(u) = self.unseen.iter().rev().find(|u| t - u.2 < 0.9 && u.3) {
                let finishing = target.map_or(false, |ti| ens[ti].hp < 250.0 && ens[ti].d < 120.0);
                if !finishing && laser_charge <= 0.0 {
                    aim = [u.1, 0.0];
                    fire = [false, false];
                }
            }
        }
        a.aim = Some([wrap(aim[0]), aim[1].clamp(me.stats.min_pitch, me.stats.max_pitch)]);
        a.fire = fire;
        if a.reload {
            // don't reload while the gun can fire at a target
            for (si, w) in me.weapons.iter().enumerate() {
                if let Some(w) = w {
                    if w.kind == WK::Gun && fire[si] && w.ammo > 0.0 {
                        a.reload = false;
                    }
                }
            }
        }

        // ---------------- weapon pickup
        let reach = self.cfg.pickup_radius;
        let mut best_take: Option<(i64, f64, Option<usize>)> = None;
        for pk in &o.pickups {
            let dh = ((pk.pos[0] - me.pos[0]).powi(2) + (pk.pos[1] - me.pos[1]).powi(2)).sqrt();
            if dh > reach - 0.3 || (pk.pos[2] - me.pos[2]).abs() > 3.8 {
                continue;
            }
            if self.blacklist.iter().any(|b| b.0 == pk.id) {
                continue;
            }
            if me.weapons.iter().flatten().any(|w| w.kind == pk.kind) {
                continue;
            }
            let v = self.p.wpref[pk.kind as usize];
            // which slot?
            let (slot, worst) = if me.weapons[1].is_none() {
                (None, 0.0)
            } else if me.weapons[0].is_none() {
                (None, 0.0)
            } else {
                let w1 = me.weapons[1].as_ref().unwrap();
                (Some(1usize), self.p.wpref[w1.kind as usize])
            };
            // never swap out a charging laser
            if let Some(s) = slot {
                if me.weapons[s].as_ref().map_or(false, |w| w.kind == WK::Laser && w.charge > 0.0) {
                    continue;
                }
            }
            let gain = v - worst;
            if gain > 0.05 && best_take.map_or(true, |b| gain > b.1) {
                best_take = Some((pk.id, gain, slot));
            }
        }
        if let Some((id, _, slot)) = best_take {
            a.take = Some(id);
            a.drop = slot;
        }

        if self.debug {
            self.label = format!(
                "{:?} hp{:.0} e{:.0} tgt{:?} eng{} zone{}",
                mode,
                me.hp,
                me.energy,
                target.map(|i| ens[i].id),
                engaged.len(),
                me.in_zone
            );
            a.label = Some(self.label.clone());
        }
        self.last_t = t;
        a
    }
}
