//! mechfight/1 protocol types: typed config, observations and actions.

use crate::json::{fnum, fstr, Value};

pub type V3 = [f64; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WK {
    Gun = 0,
    Mortar = 1,
    Laser = 2,
    Lightning = 3,
}

impl WK {
    pub fn from_str(s: &str) -> Option<WK> {
        match s {
            "gun" => Some(WK::Gun),
            "mortar" => Some(WK::Mortar),
            "laser" => Some(WK::Laser),
            "lightning" => Some(WK::Lightning),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            WK::Gun => "gun",
            WK::Mortar => "mortar",
            WK::Laser => "laser",
            WK::Lightning => "lightning",
        }
    }
    pub const ALL: [WK; 4] = [WK::Gun, WK::Mortar, WK::Laser, WK::Lightning];
}

/// Union of every weapon stat name.
#[derive(Clone, Debug, Default)]
pub struct WStats {
    pub damage: f64,
    pub cooldown: f64,
    pub speed: f64,
    pub range: f64,
    pub spread_deg: f64,
    pub mag: f64,
    pub reload: f64,
    pub splash: f64,
    pub splash_radius: f64,
    pub splash_edge: f64,
    pub gravity: f64,
    pub ttl: f64,
    pub knock: f64,
    pub knock_up: f64,
    pub drain: f64,
    pub charge: f64,
    pub min_charge: f64,
    pub min_frac: f64,
    pub hold_s: f64,
    pub pierce: f64,
    pub charge_turn_mult: f64,
    pub cookoff: f64,
    pub cone_deg: f64,
    pub chain: f64,
    pub chain_range: f64,
    pub chain_falloff: f64,
    pub energy: f64,
    pub short_s: f64,
}

impl WStats {
    pub fn from(v: &Value) -> WStats {
        let g = |k: &str| v.get(k).f();
        WStats {
            damage: g("damage"),
            cooldown: g("cooldown"),
            speed: g("speed"),
            range: g("range"),
            spread_deg: g("spread_deg"),
            mag: g("mag"),
            reload: g("reload"),
            splash: g("splash"),
            splash_radius: g("splash_radius"),
            splash_edge: g("splash_edge"),
            gravity: g("gravity"),
            ttl: g("ttl"),
            knock: g("knock"),
            knock_up: g("knock_up"),
            drain: g("drain"),
            charge: g("charge"),
            min_charge: g("min_charge"),
            min_frac: g("min_frac"),
            hold_s: g("hold_s"),
            pierce: g("pierce"),
            charge_turn_mult: v.get("charge_turn_mult").f_or(1.0),
            cookoff: g("cookoff"),
            cone_deg: g("cone_deg"),
            chain: g("chain"),
            chain_range: g("chain_range"),
            chain_falloff: g("chain_falloff"),
            energy: g("energy"),
            short_s: g("short_s"),
        }
    }
    pub fn get_mut(&mut self, k: &str) -> Option<&mut f64> {
        Some(match k {
            "damage" => &mut self.damage,
            "cooldown" => &mut self.cooldown,
            "speed" => &mut self.speed,
            "range" => &mut self.range,
            "spread_deg" => &mut self.spread_deg,
            "mag" => &mut self.mag,
            "reload" => &mut self.reload,
            "splash" => &mut self.splash,
            "splash_radius" => &mut self.splash_radius,
            "gravity" => &mut self.gravity,
            "charge" => &mut self.charge,
            "pierce" => &mut self.pierce,
            "cone_deg" => &mut self.cone_deg,
            "chain" => &mut self.chain,
            "chain_range" => &mut self.chain_range,
            "energy" => &mut self.energy,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct MStats {
    pub radius: f64,
    pub height: f64,
    pub eye_height: f64,
    pub muzzle_height: f64,
    pub hp: f64,
    pub walk_speed: f64,
    pub boost_speed: f64,
    pub ground_accel: f64,
    pub air_accel: f64,
    pub gravity: f64,
    pub jump_speed: f64,
    pub jet_accel: f64,
    pub max_rise_speed: f64,
    pub step_up: f64,
    pub max_slope_deg: f64,
    pub energy: f64,
    pub energy_regen: f64,
    pub energy_regen_delay: f64,
    pub boost_drain: f64,
    pub jet_drain: f64,
    pub turn_rate: f64,
    pub pitch_rate: f64,
    pub min_pitch: f64,
    pub max_pitch: f64,
    pub regen_delay: f64,
    pub regen_per_s: f64,
    pub charge_move_mult: f64,
    pub self_damage_mult: f64,
    pub armor: f64,
    pub rear_mult: f64,
    pub front_arc_deg: f64,
    pub near_range_bonus: f64,
    pub vision_bonus: f64,
    pub hearing: f64,
}

impl MStats {
    pub fn from(v: &Value) -> MStats {
        let g = |k: &str, d: f64| v.get(k).f_or(d);
        MStats {
            radius: g("radius", 2.5),
            height: g("height", 9.0),
            eye_height: g("eye_height", 8.0),
            muzzle_height: g("muzzle_height", 7.0),
            hp: g("hp", 1400.0),
            walk_speed: g("walk_speed", 11.7),
            boost_speed: g("boost_speed", 19.5),
            ground_accel: g("ground_accel", 58.5),
            air_accel: g("air_accel", 12.0),
            gravity: g("gravity", 22.0),
            jump_speed: g("jump_speed", 11.0),
            jet_accel: g("jet_accel", 34.0),
            max_rise_speed: g("max_rise_speed", 10.0),
            step_up: g("step_up", 1.2),
            max_slope_deg: g("max_slope_deg", 50.0),
            energy: g("energy", 100.0),
            energy_regen: g("energy_regen", 18.0),
            energy_regen_delay: g("energy_regen_delay", 0.6),
            boost_drain: g("boost_drain", 17.0),
            jet_drain: g("jet_drain", 15.0),
            turn_rate: g("turn_rate", 4.0),
            pitch_rate: g("pitch_rate", 3.0),
            min_pitch: g("min_pitch", -0.7),
            max_pitch: g("max_pitch", 1.3),
            regen_delay: g("regen_delay", 5.0),
            regen_per_s: g("regen_per_s", 45.0),
            charge_move_mult: g("charge_move_mult", 0.5),
            self_damage_mult: g("self_damage_mult", 0.5),
            armor: g("armor", 0.0),
            rear_mult: g("rear_mult", 1.5),
            front_arc_deg: g("front_arc_deg", 180.0),
            near_range_bonus: g("near_range_bonus", 0.0),
            vision_bonus: g("vision_bonus", 0.0),
            hearing: g("hearing", 1.0),
        }
    }
    pub fn get_mut(&mut self, k: &str) -> Option<&mut f64> {
        Some(match k {
            "hp" => &mut self.hp,
            "armor" => &mut self.armor,
            "walk_speed" => &mut self.walk_speed,
            "boost_speed" => &mut self.boost_speed,
            "energy" => &mut self.energy,
            "energy_regen" => &mut self.energy_regen,
            "boost_drain" => &mut self.boost_drain,
            "jet_drain" => &mut self.jet_drain,
            "regen_per_s" => &mut self.regen_per_s,
            "regen_delay" => &mut self.regen_delay,
            "turn_rate" => &mut self.turn_rate,
            "pitch_rate" => &mut self.pitch_rate,
            "near_range_bonus" => &mut self.near_range_bonus,
            "vision_bonus" => &mut self.vision_bonus,
            "hearing" => &mut self.hearing,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Effect {
    pub path: String,
    pub mul: Option<f64>,
    pub add: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct UpgradeDef {
    pub id: String,
    pub requires: Option<WK>,
    pub max: u32,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Debug)]
pub struct ZonePhase {
    pub hold_s: f64,
    pub shrink_s: f64,
    pub radius: f64,
    pub dps: f64,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub tick_hz: f64,
    pub decide_every: f64,
    pub duration_s: f64,
    pub mechs: usize,
    pub tick_budget_ms: f64,
    pub coast_s: f64,
    pub mech: MStats,
    pub vis_range: f64,
    pub fov_deg: f64,
    pub near_range: f64,
    pub memory_s: f64,
    pub show_enemy_hp: bool,
    pub bearing_noise_deg: f64,
    pub snd_near: f64,
    pub snd_mid: f64,
    pub step_speed: f64,
    pub step_every: f64,
    pub snd_range: Vec<(String, f64)>,
    pub weapons: [WStats; 4],
    pub gun_radius: f64,
    pub npc_proj_radius: f64,
    pub cube_pickup_radius: f64,
    pub hp_per_cube: f64,
    pub damage_per_cube: f64,
    pub stat_cap: f64,
    pub death_drop_extra: f64,
    pub size_per_cube: f64,
    pub upgrade_thresholds: Vec<f64>,
    pub crate_count: usize,
    pub crate_roof_fraction: f64,
    pub crate_hp: f64,
    pub crate_radius: f64,
    pub crate_height: f64,
    pub crate_cubes: f64,
    pub pickup_radius: f64,
    pub equip_s: f64,
    pub floor_count: usize,
    pub floor_roof_fraction: f64,
    pub crate_weapon_chance: f64,
    pub loot_weights: [f64; 4],
    pub npc_count: usize,
    pub npc_hp: f64,
    pub npc_radius: f64,
    pub npc_height: f64,
    pub npc_speed: f64,
    pub npc_leash: f64,
    pub npc_chase_leash: f64,
    pub npc_aggro: f64,
    pub npc_deaggro: f64,
    pub npc_damage: f64,
    pub npc_cooldown: f64,
    pub npc_proj_speed: f64,
    pub npc_spread_deg: f64,
    pub npc_cubes: f64,
    pub drop_times: Vec<f64>,
    pub drop_fall_s: f64,
    pub drop_radius: f64,
    pub drop_capture_s: f64,
    pub drop_cubes: f64,
    pub drop_spread: f64,
    pub zone_start_center: [f64; 2],
    pub zone_start_radius: f64,
    pub zone_phases: Vec<ZonePhase>,
    pub upgrades: Vec<UpgradeDef>,
    pub spawn_min_sep: f64,
    pub spawn_zone_fraction: f64,
    pub arena_margin: f64,
}

impl Config {
    pub fn from(c: &Value) -> Config {
        let m = c.get("match");
        let b = c.get("bot");
        let vi = c.get("vision");
        let so = c.get("sound");
        let w = c.get("weapons");
        let cu = c.get("cubes");
        let cr = c.get("crates");
        let lo = c.get("loot");
        let np = c.get("npc");
        let dr = c.get("drops");
        let zo = c.get("zone");
        let lw = lo.get("weights");
        Config {
            tick_hz: m.get("tick_hz").f_or(20.0),
            decide_every: m.get("decide_every").f_or(2.0),
            duration_s: m.get("duration_s").f_or(480.0),
            mechs: m.get("mechs").f_or(16.0) as usize,
            tick_budget_ms: b.get("tick_budget_ms").f_or(50.0),
            coast_s: b.get("coast_s").f_or(1.0),
            mech: MStats::from(c.get("mech")),
            vis_range: vi.get("range").f_or(200.0),
            fov_deg: vi.get("fov_deg").f_or(120.0),
            near_range: vi.get("near_range").f_or(45.0),
            memory_s: vi.get("memory_s").f_or(5.0),
            show_enemy_hp: !matches!(vi.get("show_enemy_hp"), Value::Bool(false)),
            bearing_noise_deg: so.get("bearing_noise_deg").f_or(7.0),
            snd_near: so.get("near").f_or(60.0),
            snd_mid: so.get("mid").f_or(160.0),
            step_speed: so.get("step_speed").f_or(5.0),
            step_every: so.get("step_every").f_or(0.6),
            snd_range: so.get("range").obj().iter().map(|(k, v)| (k.clone(), v.f())).collect(),
            weapons: [
                WStats::from(w.get("gun")),
                WStats::from(w.get("mortar")),
                WStats::from(w.get("laser")),
                WStats::from(w.get("lightning")),
            ],
            gun_radius: c.get("projectile").get("gun_radius").f_or(0.3),
            npc_proj_radius: c.get("projectile").get("npc_radius").f_or(0.3),
            cube_pickup_radius: cu.get("pickup_radius").f_or(3.5),
            hp_per_cube: cu.get("hp_per_cube").f_or(0.04),
            damage_per_cube: cu.get("damage_per_cube").f_or(0.04),
            stat_cap: cu.get("stat_cap").f_or(10.0),
            death_drop_extra: cu.get("death_drop_extra").f_or(2.0),
            size_per_cube: cu.get("size_per_cube").f_or(0.05),
            upgrade_thresholds: cu.get("upgrade_thresholds").arr().iter().map(|v| v.f()).collect(),
            crate_count: cr.get("count").f_or(36.0) as usize,
            crate_roof_fraction: cr.get("roof_fraction").f_or(0.25),
            crate_hp: cr.get("hp").f_or(300.0),
            crate_radius: cr.get("radius").f_or(1.4),
            crate_height: cr.get("height").f_or(2.4),
            crate_cubes: cr.get("cubes").f_or(1.0),
            pickup_radius: lo.get("pickup_radius").f_or(4.0),
            equip_s: lo.get("equip_s").f_or(1.0),
            floor_count: lo.get("floor_count").f_or(40.0) as usize,
            floor_roof_fraction: lo.get("floor_roof_fraction").f_or(0.25),
            crate_weapon_chance: lo.get("crate_weapon_chance").f_or(0.5),
            loot_weights: [
                lw.get("gun").f_or(0.0),
                lw.get("mortar").f_or(1.0),
                lw.get("laser").f_or(1.0),
                lw.get("lightning").f_or(1.0),
            ],
            npc_count: np.get("count").f_or(12.0) as usize,
            npc_hp: np.get("hp").f_or(350.0),
            npc_radius: np.get("radius").f_or(1.8),
            npc_height: np.get("height").f_or(3.5),
            npc_speed: np.get("speed").f_or(5.0),
            npc_leash: np.get("leash").f_or(30.0),
            npc_chase_leash: np.get("chase_leash").f_or(60.0),
            npc_aggro: np.get("aggro_range").f_or(45.0),
            npc_deaggro: np.get("deaggro_range").f_or(90.0),
            npc_damage: np.get("damage").f_or(25.0),
            npc_cooldown: np.get("cooldown").f_or(1.0),
            npc_proj_speed: np.get("projectile_speed").f_or(90.0),
            npc_spread_deg: np.get("spread_deg").f_or(2.5),
            npc_cubes: np.get("cubes").f_or(2.0),
            drop_times: dr.get("times").arr().iter().map(|v| v.f()).collect(),
            drop_fall_s: dr.get("fall_s").f_or(25.0),
            drop_radius: dr.get("radius").f_or(8.0),
            drop_capture_s: dr.get("capture_s").f_or(4.0),
            drop_cubes: dr.get("cubes").f_or(3.0),
            drop_spread: dr.get("spread").f_or(0.7),
            zone_start_center: zo.get("start_center").v2(),
            zone_start_radius: zo.get("start_radius").f_or(560.0),
            zone_phases: zo
                .get("phases")
                .arr()
                .iter()
                .map(|p| ZonePhase {
                    hold_s: p.get("hold_s").f(),
                    shrink_s: p.get("shrink_s").f(),
                    radius: p.get("radius").f(),
                    dps: p.get("dps").f(),
                })
                .collect(),
            upgrades: c
                .get("upgrades")
                .arr()
                .iter()
                .map(|u| UpgradeDef {
                    id: u.get("id").s().to_string(),
                    requires: WK::from_str(u.get("requires").s()),
                    max: u.get("max").f_or(1.0) as u32,
                    effects: u
                        .get("effects")
                        .arr()
                        .iter()
                        .map(|e| Effect {
                            path: e.get("path").s().to_string(),
                            mul: match e.get("mul") {
                                Value::Num(n) => Some(*n),
                                _ => None,
                            },
                            add: match e.get("add") {
                                Value::Num(n) => Some(*n),
                                _ => None,
                            },
                        })
                        .collect(),
                })
                .collect(),
            spawn_min_sep: m.get("spawn_min_separation").f_or(130.0),
            spawn_zone_fraction: m.get("spawn_zone_fraction").f_or(0.8),
            arena_margin: m.get("arena_margin").f_or(30.0),
        }
    }
    pub fn w(&self, k: WK) -> &WStats {
        &self.weapons[k as usize]
    }
    pub fn sound_range(&self, kind: &str) -> f64 {
        for (k, v) in &self.snd_range {
            if k == kind {
                return *v;
            }
        }
        100.0
    }
}

#[derive(Clone, Debug)]
pub struct Weapon {
    pub kind: WK,
    pub cooldown: f64,
    pub ammo: f64,
    pub reload: f64,
    pub charge: f64,
    pub stats: WStats,
}

#[derive(Clone, Debug)]
pub struct SelfObs {
    pub id: i32,
    pub pos: V3,
    pub vel: V3,
    pub aim: [f64; 2],
    pub hp: f64,
    pub max_hp: f64,
    pub energy: f64,
    pub max_energy: f64,
    pub grounded: bool,
    pub cubes: f64,
    pub upgrades: Vec<String>,
    pub weapons: [Option<Weapon>; 2],
    pub in_zone: bool,
    pub stats: MStats,
}

#[derive(Clone, Debug)]
pub struct Actor {
    pub id: i32,
    pub is_mech: bool,
    pub pos: V3,
    pub vel: V3,
    pub aim: [f64; 2],
    pub hp: f64,
    pub max_hp: f64,
    pub cubes: f64,
    pub weapons: Vec<WK>,
    pub charging: bool,
}

#[derive(Clone, Debug)]
pub struct Proj {
    pub id: i64,
    pub kind: u8, // 0 gun, 1 mortar, 2 npc
    pub pos: V3,
    pub vel: V3,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: i64,
    pub pos: V3,
    pub hp: f64,
}

#[derive(Clone, Debug)]
pub struct Pickup {
    pub id: i64,
    pub kind: WK,
    pub pos: V3,
}

#[derive(Clone, Debug)]
pub struct Sound {
    pub kind: String,
    pub bearing: f64,
    pub dist: u8, // 0 near 1 mid 2 far
    pub age: f64,
}

#[derive(Clone, Debug)]
pub struct Beam {
    pub by: i32,
    pub laser: bool,
    pub path: Vec<V3>,
    pub age: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Event {
    pub typ: String,
    pub amount: f64,
    pub weapon: String,
    pub bearing: Option<f64>,
    pub by: Option<i32>,
    pub target: i64,
    pub rear: bool,
    pub id: i64,
    pub kind: String,
    pub slot: i32,
    pub reason: String,
    pub cubes: f64,
    pub effect: String,
}

#[derive(Clone, Debug)]
pub struct Drop {
    pub id: i64,
    pub pos: V3,
    pub radius: f64,
    pub lands_in: f64,
    pub capture_s: f64,
    pub progress: f64,
    pub holder: Option<i32>,
}

#[derive(Clone, Debug, Default)]
pub struct Zone {
    pub center: [f64; 2],
    pub radius: f64,
    pub next_center: [f64; 2],
    pub next_radius: f64,
    pub shrink_start: f64,
    pub shrink_end: f64,
    pub dps: f64,
}

#[derive(Clone, Debug)]
pub struct Offer {
    pub id: i64,
    pub choices: Vec<String>,
    pub expires: f64,
}

#[derive(Clone, Debug)]
pub struct Obs {
    pub tick: i64,
    pub t: f64,
    pub alive: i32,
    pub me: SelfObs,
    pub mechs: Vec<Actor>,
    pub projectiles: Vec<Proj>,
    pub crates: Vec<Item>,
    pub cubes: Vec<Item>,
    pub pickups: Vec<Pickup>,
    pub sounds: Vec<Sound>,
    pub beams: Vec<Beam>,
    pub events: Vec<Event>,
    pub drops: Vec<Drop>,
    pub zone: Zone,
    pub offer: Option<Offer>,
}

fn opt_i(v: &Value) -> Option<i32> {
    match v {
        Value::Num(n) => Some(*n as i32),
        _ => None,
    }
}

impl Obs {
    pub fn from(o: &Value) -> Obs {
        let s = o.get("self");
        let mut weapons: [Option<Weapon>; 2] = [None, None];
        for (i, w) in s.get("weapons").arr().iter().enumerate().take(2) {
            if let Some(k) = WK::from_str(w.get("kind").s()) {
                weapons[i] = Some(Weapon {
                    kind: k,
                    cooldown: w.get("cooldown").f(),
                    ammo: w.get("ammo").f(),
                    reload: w.get("reload").f(),
                    charge: w.get("charge").f(),
                    stats: WStats::from(w.get("stats")),
                });
            }
        }
        let me = SelfObs {
            id: s.get("id").f() as i32,
            pos: s.get("pos").v3(),
            vel: s.get("vel").v3(),
            aim: s.get("aim").v2(),
            hp: s.get("hp").f(),
            max_hp: s.get("max_hp").f(),
            energy: s.get("energy").f(),
            max_energy: s.get("max_energy").f(),
            grounded: s.get("grounded").b(),
            cubes: s.get("cubes").f(),
            upgrades: s.get("upgrades").arr().iter().map(|u| u.s().to_string()).collect(),
            weapons,
            in_zone: s.get("in_zone").b(),
            stats: MStats::from(s.get("stats")),
        };
        let mechs = o
            .get("mechs")
            .arr()
            .iter()
            .map(|m| Actor {
                id: m.get("id").f() as i32,
                is_mech: m.get("kind").s() == "mech",
                pos: m.get("pos").v3(),
                vel: m.get("vel").v3(),
                aim: m.get("aim").v2(),
                hp: m.get("hp").f_or(-1.0),
                max_hp: m.get("max_hp").f_or(-1.0),
                cubes: m.get("cubes").f(),
                weapons: m.get("weapons").arr().iter().filter_map(|w| WK::from_str(w.s())).collect(),
                charging: m.get("charging").b(),
            })
            .collect();
        let projectiles = o
            .get("projectiles")
            .arr()
            .iter()
            .map(|p| Proj {
                id: p.get("id").f() as i64,
                kind: match p.get("kind").s() {
                    "gun" => 0,
                    "mortar" => 1,
                    _ => 2,
                },
                pos: p.get("pos").v3(),
                vel: p.get("vel").v3(),
            })
            .collect();
        let items = |k: &str| -> Vec<Item> {
            o.get(k)
                .arr()
                .iter()
                .map(|p| Item { id: p.get("id").f() as i64, pos: p.get("pos").v3(), hp: p.get("hp").f() })
                .collect()
        };
        let pickups = o
            .get("pickups")
            .arr()
            .iter()
            .filter_map(|p| {
                WK::from_str(p.get("kind").s()).map(|k| Pickup { id: p.get("id").f() as i64, kind: k, pos: p.get("pos").v3() })
            })
            .collect();
        let sounds = o
            .get("sounds")
            .arr()
            .iter()
            .map(|p| Sound {
                kind: p.get("kind").s().to_string(),
                bearing: p.get("bearing").f(),
                dist: match p.get("dist").s() {
                    "near" => 0,
                    "mid" => 1,
                    _ => 2,
                },
                age: p.get("age").f(),
            })
            .collect();
        let beams = o
            .get("beams")
            .arr()
            .iter()
            .map(|p| Beam {
                by: p.get("by").f() as i32,
                laser: p.get("kind").s() == "laser",
                path: p.get("path").arr().iter().map(|q| q.v3()).collect(),
                age: p.get("age").f(),
            })
            .collect();
        let events = o
            .get("events")
            .arr()
            .iter()
            .map(|e| Event {
                typ: e.get("type").s().to_string(),
                amount: e.get("amount").f(),
                weapon: e.get("weapon").s().to_string(),
                bearing: match e.get("bearing") {
                    Value::Num(n) => Some(*n),
                    _ => None,
                },
                by: opt_i(e.get("by")),
                target: e.get("target").f() as i64,
                rear: e.get("rear").b(),
                id: e.get("id").f() as i64,
                kind: e.get("kind").s().to_string(),
                slot: e.get("slot").f() as i32,
                reason: e.get("reason").s().to_string(),
                cubes: e.get("cubes").f(),
                effect: e.get("effect").s().to_string(),
            })
            .collect();
        let drops = o
            .get("drops")
            .arr()
            .iter()
            .map(|d| Drop {
                id: d.get("id").f() as i64,
                pos: d.get("pos").v3(),
                radius: d.get("radius").f_or(8.0),
                lands_in: d.get("lands_in").f(),
                capture_s: d.get("capture_s").f_or(4.0),
                progress: d.get("progress").f(),
                holder: opt_i(d.get("holder")),
            })
            .collect();
        let z = o.get("zone");
        let zone = Zone {
            center: z.get("center").v2(),
            radius: z.get("radius").f(),
            next_center: z.get("next_center").v2(),
            next_radius: z.get("next_radius").f(),
            shrink_start: z.get("shrink_start").f(),
            shrink_end: z.get("shrink_end").f(),
            dps: z.get("dps").f(),
        };
        let offer = match o.get("offer") {
            Value::Obj(_) => {
                let of = o.get("offer");
                Some(Offer {
                    id: of.get("id").f() as i64,
                    choices: of.get("choices").arr().iter().map(|c| c.get("id").s().to_string()).collect(),
                    expires: of.get("expires").f(),
                })
            }
            _ => None,
        };
        Obs {
            tick: o.get("tick").f() as i64,
            t: o.get("t").f(),
            alive: o.get("alive").f() as i32,
            me,
            mechs,
            projectiles,
            crates: items("crates"),
            cubes: items("cubes"),
            pickups,
            sounds,
            beams,
            events,
            drops,
            zone,
            offer,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Action {
    pub mv: [f64; 2],
    pub boost: bool,
    pub jump: bool,
    pub aim: Option<[f64; 2]>,
    pub fire: [bool; 2],
    pub reload: bool,
    pub pick: Option<usize>,
    pub take: Option<i64>,
    pub drop: Option<usize>,
    pub label: Option<String>,
}

impl Action {
    pub fn to_json(&self, tick: i64) -> String {
        let mut s = String::with_capacity(200);
        s.push_str("{\"tick\":");
        s.push_str(&tick.to_string());
        let (mx, my) = (self.mv[0], self.mv[1]);
        let l = (mx * mx + my * my).sqrt();
        let (mx, my) = if l > 1.0 { (mx / l, my / l) } else { (mx, my) };
        s.push_str(&format!(",\"move\":[{},{}]", fnum(mx), fnum(my)));
        s.push_str(&format!(",\"boost\":{},\"jump\":{}", self.boost, self.jump));
        if let Some(a) = self.aim {
            s.push_str(&format!(",\"aim\":[{},{}]", fnum(a[0]), fnum(a[1])));
        }
        s.push_str(&format!(",\"fire\":[{},{}]", self.fire[0], self.fire[1]));
        if self.reload {
            s.push_str(",\"reload\":true");
        }
        if let Some(p) = self.pick {
            s.push_str(&format!(",\"pick\":{}", p));
        }
        if let Some(t) = self.take {
            s.push_str(&format!(",\"take\":{}", t));
        }
        if let Some(d) = self.drop {
            s.push_str(&format!(",\"drop\":{}", d));
        }
        if let Some(lb) = &self.label {
            let lb: String = lb.chars().take(78).collect();
            s.push_str(&format!(",\"debug\":{{\"label\":{}}}", fstr(&lb)));
        }
        s.push('}');
        s
    }
}
