//! Opponent agents for the local simulator (approximations, not the real house bots).

use crate::bot::{Bot, Params};
use crate::geom::*;
use crate::nav::Nav;
use crate::proto::*;
use crate::sim::Agent;
use crate::world::World;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub struct BotAgent {
    pub bot: Bot,
    pub tag: String,
}
impl Agent for BotAgent {
    fn act(&mut self, o: &Obs) -> Action {
        if self.tag == "me" && std::env::var("SIMTRACE").is_ok() {
            for e in &o.events {
                if e.typ == "damaged" || e.typ == "kill" || e.typ == "took" {
                    eprintln!("t={:.1} {} {} {:.0} by {:?} hp={:.0} pos={:?}", o.t, e.typ, e.weapon, e.amount, e.by, o.me.hp, o.me.pos.map(|v| v.round()));
                }
            }
            if o.tick % 40 == 0 {
                eprintln!("t={:.1} mode={:?} hp={:.0} pos={:?} vis={:?}", o.t, self.bot.mode, o.me.hp, o.me.pos.map(|v| v.round()), o.mechs.iter().filter(|m| m.is_mech).map(|m| (m.id, d3(&m.pos, &o.me.pos).round())).collect::<Vec<_>>());
            }
        }
        self.bot.act(o)
    }
    fn name(&self) -> String {
        self.tag.clone()
    }
}

pub use crate::sim::Intel;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Skill {
    Easy,
    Medium,
    Hard,
    Expert,
}

pub struct House {
    pub id: i32,
    pub skill: Skill,
    pub nav: Nav,
    pub cfg: Config,
    pub world: Arc<World>,
    pub rng: Rng,
    pub intel: Rc<RefCell<Intel>>,
    pub squad_ids: Vec<i32>,
    path: Vec<(f64, f64)>,
    path_goal: (f64, f64),
    path_t: f64,
    strafe: f64,
    strafe_t: f64,
    aim_err: (f64, f64),
    aim_err_t: f64,
    react_t: f64,
    target_id: i32,
    roam: Option<(f64, f64)>,
}

impl House {
    pub fn new(id: i32, skill: Skill, nav: Nav, cfg: Config, world: Arc<World>, seed: u64, intel: Rc<RefCell<Intel>>, squad_ids: Vec<i32>) -> House {
        House {
            id,
            skill,
            nav,
            cfg,
            world,
            rng: Rng(seed | 1),
            intel,
            squad_ids,
            path: vec![],
            path_goal: (1e9, 1e9),
            path_t: -100.0,
            strafe: 1.0,
            strafe_t: 0.0,
            aim_err: (0.0, 0.0),
            aim_err_t: 0.0,
            react_t: 0.0,
            target_id: -1,
            roam: None,
        }
    }
    fn sigma(&self) -> f64 {
        let d: f64 = match self.skill {
            Skill::Easy => 3.5,
            Skill::Medium => 2.0,
            Skill::Hard => 1.0,
            Skill::Expert => 0.45,
        };
        d.to_radians()
    }
    fn react(&self) -> f64 {
        match self.skill {
            Skill::Easy => 0.7,
            Skill::Medium => 0.45,
            Skill::Hard => 0.3,
            Skill::Expert => 0.15,
        }
    }
    fn nav_to(&mut self, o: &Obs, gx: f64, gy: f64) -> [f64; 2] {
        let me = &o.me;
        let (x, y) = (me.pos[0], me.pos[1]);
        if (gx - x).powi(2) + (gy - y).powi(2) < 4.0 {
            return [0.0, 0.0];
        }
        if (gx - self.path_goal.0).abs() + (gy - self.path_goal.1).abs() > 8.0 || o.t - self.path_t > 3.0 || self.path.is_empty() {
            self.path = self.nav.path(x, y, me.pos[2], gx, gy, 40000, &|_| 0.0).unwrap_or_default();
            self.path_goal = (gx, gy);
            self.path_t = o.t;
        }
        while self.path.len() > 1 && ((self.path[0].0 - x).powi(2) + (self.path[0].1 - y).powi(2)) < 9.0 {
            self.path.remove(0);
        }
        let tgt = if self.path.is_empty() {
            (gx, gy)
        } else {
            let mut t = self.path[0];
            for i in (0..self.path.len().min(15)).rev() {
                let p = self.path[i];
                if (p.0 - x).powi(2) + (p.1 - y).powi(2) < 1200.0 && self.nav.walkable_line(x, y, p.0, p.1) {
                    t = p;
                    break;
                }
            }
            t
        };
        let (nx, ny) = norm2(tgt.0 - x, tgt.1 - y);
        [nx, ny]
    }
}

fn zone_inside(o: &Obs, t_ahead: f64) -> ([f64; 2], f64) {
    let z = &o.zone;
    let t = o.t + t_ahead;
    if t <= z.shrink_start {
        return (z.center, z.radius);
    }
    let f = ((t - z.shrink_start) / (z.shrink_end - z.shrink_start).max(1e-6)).clamp(0.0, 1.0);
    ([z.center[0] + (z.next_center[0] - z.center[0]) * f, z.center[1] + (z.next_center[1] - z.center[1]) * f], z.radius + (z.next_radius - z.radius) * f)
}

impl Agent for House {
    fn name(&self) -> String {
        format!("house-{:?}", self.skill)
    }
    fn act(&mut self, o: &Obs) -> Action {
        let me = &o.me;
        let mut a = Action::default();
        if o.offer.is_some() {
            a.pick = Some(0);
        }
        let intel = self.intel.borrow().clone();
        let _ = intel.t;
        let hunting = intel.hunters.contains(&me.id);
        // enemies: any non-squad mech
        let enemies: Vec<&Actor> = o.mechs.iter().filter(|m| m.is_mech && !self.squad_ids.contains(&m.id)).collect();
        let mut target: Option<(i32, V3, V3, f64)> = None;
        for e in &enemies {
            let d = d3(&e.pos, &me.pos);
            if target.map_or(true, |t| d < t.3) {
                target = Some((e.id, e.pos, e.vel, d));
            }
        }
        // shared vision: hunters know prey position
        if target.is_none() && hunting {
            if let Some((pid, pp, pv)) = intel.prey {
                target = Some((pid, pp, pv, d3(&pp, &me.pos)));
            }
        }
        let muz = [me.pos[0], me.pos[1], me.pos[2] + me.stats.muzzle_height];
        // movement
        let (zc, zr) = zone_inside(o, 10.0);
        let dz = d2(&me.pos, &zc);
        if dz > zr * 0.9 {
            a.mv = self.nav_to(o, zc[0], zc[1]);
            a.boost = !me.in_zone;
        } else if let Some((tid, tp, _, d)) = target {
            let visible = enemies.iter().any(|e| e.id == tid);
            if hunting || (visible && d < 120.0) {
                let pref = if me.weapons.iter().flatten().any(|w| w.kind == WK::Lightning) { 25.0 } else { 55.0 };
                let los = self.world.los(muz, [tp[0], tp[1], tp[2] + 4.5]);
                if !los || d > pref + 50.0 {
                    a.mv = self.nav_to(o, tp[0], tp[1]);
                    a.boost = d > 150.0 && me.energy > 50.0;
                } else {
                    let (rx, ry) = norm2(tp[0] - me.pos[0], tp[1] - me.pos[1]);
                    if o.t > self.strafe_t {
                        self.strafe = if self.rng.f() < 0.5 { 1.0 } else { -1.0 };
                        self.strafe_t = o.t + self.rng.range(0.5, 1.5);
                    }
                    let radial = ((d - pref) / 30.0).clamp(-1.0, 1.0);
                    let s = match self.skill {
                        Skill::Easy => 0.3,
                        Skill::Medium => 0.6,
                        _ => 0.9,
                    };
                    let (nx, ny) = norm2(-ry * self.strafe * s + rx * radial, rx * self.strafe * s + ry * radial);
                    a.mv = [nx, ny];
                }
            } else {
                a.mv = self.loot_or_roam(o);
            }
        } else {
            a.mv = self.loot_or_roam(o);
        }
        // aim & fire
        if let Some((tid, tp, tv, d)) = target {
            if tid != self.target_id {
                self.target_id = tid;
                self.react_t = o.t + self.react();
            }
            if o.t > self.aim_err_t {
                let s = self.sigma();
                self.aim_err = (self.rng.gauss() * s, self.rng.gauss() * s * 0.6);
                self.aim_err_t = o.t + 0.3;
            }
            let tc = [tp[0], tp[1], tp[2] + 4.5];
            let lead_f = match self.skill {
                Skill::Easy => 0.4,
                Skill::Medium => 0.75,
                _ => 1.0,
            };
            let gun_speed = me.weapons.iter().flatten().find(|w| w.kind == WK::Gun).map_or(252.0, |w| w.stats.speed);
            let ti = intercept(muz, tc, tv, gun_speed).unwrap_or(d / gun_speed);
            let ap = add(tc, mul(tv, ti * lead_f));
            let (ay, apt) = angles(sub(ap, muz));
            let aim = [ay + self.aim_err.0, apt + self.aim_err.1];
            a.aim = Some(aim);
            let visible = enemies.iter().any(|e| e.id == tid);
            let los = self.world.los(muz, tc);
            if visible && los && o.t >= self.react_t {
                let err = wrap(aim[0] - me.aim[0]).abs();
                for (s, w) in me.weapons.iter().enumerate() {
                    if let Some(w) = w {
                        match w.kind {
                            WK::Gun => {
                                if err < 0.06 && d < w.stats.range && w.ammo > 0.0 {
                                    a.fire[s] = true;
                                }
                                if w.ammo <= 0.0 {
                                    a.reload = true;
                                }
                            }
                            WK::Laser => {
                                if d < w.stats.range {
                                    if w.charge <= 0.0 {
                                        a.fire[s] = w.cooldown <= 0.0 && err < 0.3;
                                    } else if w.charge < w.stats.charge {
                                        a.fire[s] = true;
                                    } else {
                                        a.fire[s] = err > 0.02;
                                    }
                                }
                            }
                            WK::Lightning => {
                                if d < w.stats.range && err < 0.25 {
                                    a.fire[s] = true;
                                }
                            }
                            WK::Mortar => {
                                let dxy = d2(&me.pos, &tp);
                                if dxy > 20.0 && dxy < 220.0 {
                                    if let Some(th) = ballistic_pitch(dxy, tp[2] + 1.0 - (me.pos[2] + 9.5), w.stats.speed, w.stats.gravity, false) {
                                        // only mortar-aim if it's the only weapon option
                                        if !me.weapons.iter().flatten().any(|x| x.kind == WK::Gun) {
                                            a.aim = Some([ay, th]);
                                        }
                                        if (th - me.aim[1]).abs() < 0.05 && err < 0.05 {
                                            a.fire[s] = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            let (mx, my) = (a.mv[0], a.mv[1]);
            if mx.abs() + my.abs() > 0.1 {
                a.aim = Some([my.atan2(mx), 0.0]);
            }
            // shoot crates / npcs nearby
            let mut best: Option<(V3, f64)> = None;
            for c in &o.crates {
                let d = d3(&c.pos, &me.pos);
                if d < 50.0 && best.map_or(true, |b| d < b.1) {
                    best = Some(([c.pos[0], c.pos[1], c.pos[2] + 1.2], d));
                }
            }
            if let Some((p, _)) = best {
                if self.world.los(muz, p) {
                    let (ay, apt) = angles(sub(p, muz));
                    a.aim = Some([ay, apt]);
                    if wrap(ay - me.aim[0]).abs() < 0.03 {
                        a.fire[0] = me.weapons[0].as_ref().map_or(false, |w| w.kind == WK::Gun);
                    }
                }
            }
        }
        // take weapons
        for pk in &o.pickups {
            if d2(&pk.pos, &me.pos) < 3.8 && (pk.pos[2] - me.pos[2]).abs() < 3.8 && !me.weapons.iter().flatten().any(|w| w.kind == pk.kind) && me.weapons[1].is_none() {
                a.take = Some(pk.id);
                break;
            }
        }
        a
    }
}

impl House {
    fn loot_or_roam(&mut self, o: &Obs) -> [f64; 2] {
        let me = &o.me;
        if me.weapons[1].is_none() {
            let mut best: Option<(f64, f64, f64)> = None;
            for pk in &o.pickups {
                let d = d3(&pk.pos, &me.pos);
                if (pk.pos[2] - me.pos[2]).abs() > 3.0 {
                    continue;
                }
                if d < 120.0 && best.map_or(true, |b| d < b.2) {
                    best = Some((pk.pos[0], pk.pos[1], d));
                }
            }
            if let Some((x, y, _)) = best {
                return self.nav_to(o, x, y);
            }
        }
        // roam toward next zone center area
        let z = &o.zone;
        let need_new = match self.roam {
            None => true,
            Some(r) => d2(&[r.0, r.1], &me.pos) < 8.0 || d2(&[r.0, r.1], &z.next_center) > z.next_radius * 0.8,
        };
        if need_new {
            let a = self.rng.range(-3.14159, 3.14159);
            let rr = z.next_radius * 0.6 * self.rng.f().sqrt();
            self.roam = Some((z.next_center[0] + rr * a.cos(), z.next_center[1] + rr * a.sin()));
        }
        let r = self.roam.unwrap();
        let mut m = self.nav_to(o, r.0, r.1);
        m[0] *= 0.6;
        m[1] *= 0.6;
        m
    }
}

/// Clone of the kit's example bot: walk to the next zone center, grab a weapon, shoot nearest mech.
pub struct Starter {
    pub reach: f64,
}
impl Agent for Starter {
    fn name(&self) -> String {
        "starter".into()
    }
    fn act(&mut self, o: &Obs) -> Action {
        let me = &o.me;
        let (x, y, z) = (me.pos[0], me.pos[1], me.pos[2]);
        let goal = o.zone.next_center;
        let mut a = Action::default();
        if o.offer.is_some() {
            a.pick = Some(0);
        }
        let near: Vec<&Pickup> = o.pickups.iter().filter(|p| d2(&p.pos, &[x, y]) < self.reach).collect();
        if me.weapons[1].is_none() && !near.is_empty() && near[0].kind != WK::Gun {
            a.take = Some(near[0].id);
        }
        let mut en: Vec<&Actor> = o.mechs.iter().filter(|m| m.is_mech).collect();
        en.sort_by(|a, b| d3(&a.pos, &me.pos).partial_cmp(&d3(&b.pos, &me.pos)).unwrap());
        let yaw = (goal[1] - y).atan2(goal[0] - x);
        a.mv = [yaw.cos(), yaw.sin()];
        a.aim = Some([yaw, 0.0]);
        a.boost = !me.in_zone;
        a.jump = o.tick % 40 == 0;
        if let Some(e) = en.first() {
            let t = e.pos;
            a.aim = Some([(t[1] - y).atan2(t[0] - x), (t[2] - z - 2.5).atan2(d2(&t, &[x, y]))]);
            a.fire = [true, true];
        }
        a
    }
}

/// Aggressive brawler: chases the nearest visible mech, leads shots, strafes.
pub struct Brawler {
    pub nav: Nav,
    pub world: Arc<World>,
    pub rng: Rng,
    path: Vec<(f64, f64)>,
    path_t: f64,
}
impl Brawler {
    pub fn new(nav: Nav, world: Arc<World>, seed: u64) -> Brawler {
        Brawler { nav, world, rng: Rng(seed | 1), path: vec![], path_t: -100.0 }
    }
}
impl Agent for Brawler {
    fn name(&self) -> String {
        "brawler".into()
    }
    fn act(&mut self, o: &Obs) -> Action {
        let me = &o.me;
        let mut a = Action::default();
        if o.offer.is_some() {
            a.pick = Some(0);
        }
        for pk in &o.pickups {
            if d2(&pk.pos, &me.pos) < 3.8 && me.weapons[1].is_none() && pk.kind != WK::Mortar {
                a.take = Some(pk.id);
            }
        }
        let muz = [me.pos[0], me.pos[1], me.pos[2] + me.stats.muzzle_height];
        let mut tgt: Option<&Actor> = None;
        for e in o.mechs.iter().filter(|m| m.is_mech) {
            if tgt.map_or(true, |t| d3(&e.pos, &me.pos) < d3(&t.pos, &me.pos)) {
                tgt = Some(e);
            }
        }
        let (zc, zr) = zone_inside(o, 8.0);
        let goal = if d2(&me.pos, &zc) > zr * 0.85 { (zc[0], zc[1]) } else if let Some(e) = tgt { (e.pos[0], e.pos[1]) } else { (o.zone.next_center[0], o.zone.next_center[1]) };
        if o.t - self.path_t > 2.0 || self.path.is_empty() {
            self.path = self.nav.path(me.pos[0], me.pos[1], me.pos[2], goal.0, goal.1, 30000, &|_| 0.0).unwrap_or_default();
            self.path_t = o.t;
        }
        while self.path.len() > 1 && d2(&[self.path[0].0, self.path[0].1], &me.pos) < 3.0 {
            self.path.remove(0);
        }
        if let Some(p) = self.path.first() {
            let (nx, ny) = norm2(p.0 - me.pos[0], p.1 - me.pos[1]);
            a.mv = [nx, ny];
        }
        if let Some(e) = tgt {
            let tc = [e.pos[0], e.pos[1], e.pos[2] + 4.5];
            let ti = intercept(muz, tc, e.vel, 252.0).unwrap_or(0.3);
            let (ay, ap) = angles(sub(add(tc, mul(e.vel, ti)), muz));
            a.aim = Some([ay + self.rng.gauss() * 0.01, ap]);
            let d = d3(&e.pos, &me.pos);
            if d < 50.0 {
                let (rx, ry) = norm2(tc[0] - me.pos[0], tc[1] - me.pos[1]);
                a.mv = [-ry * 0.8 + rx * 0.3, rx * 0.8 + ry * 0.3];
            }
            if wrap(ay - me.aim[0]).abs() < 0.05 && self.world.los(muz, tc) {
                for (s, w) in me.weapons.iter().enumerate() {
                    if let Some(w) = w {
                        a.fire[s] = match w.kind {
                            WK::Laser => w.charge < w.stats.charge,
                            WK::Lightning => d < 44.0,
                            _ => true,
                        };
                    }
                }
            }
        }
        a
    }
}

pub fn mk_bot(cfg: &Config, world: &Arc<World>, nav: &Nav, seed: u64, id: i32, p: Params, tag: &str) -> Box<dyn Agent> {
    Box::new(BotAgent { bot: Bot::new(cfg.clone(), world.clone(), nav.clone(), seed, id, p), tag: tag.to_string() })
}
