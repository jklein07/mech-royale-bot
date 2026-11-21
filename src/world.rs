//! Static map: terrain heightmap and rasterized prisms (buildings, stadium, bridges).

use crate::json;
use crate::proto::V3;

pub const NEG: f32 = -1.0e9;

#[derive(Clone)]
pub struct Prism {
    pub kind: u8, // 0 building, 1 stadium, 2 bridge
    pub base: f64,
    pub top: f64,
    pub ring: Vec<[f64; 2]>,
    pub holes: Vec<Vec<[f64; 2]>>,
    pub bbox: [f64; 4],
}

#[derive(Clone)]
pub struct Tree {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub height: f64,
    pub crown_r: f64,
    pub crown_base: f64,
    pub trunk_r: f64,
    pub cx: f64,
    pub cy: f64,
}

pub struct World {
    pub nx: usize,
    pub ny: usize,
    pub min_x: f64,
    pub min_y: f64,
    pub bounds: [f64; 4], // minX maxX minY maxY
    /// terrain heights at grid vertices (1 m)
    pub terrain: Vec<f32>,
    /// max top of building/stadium prisms covering a cell (NEG if none); cell (i,j) covers [minX+i-0.5, minX+i+0.5)
    pub roof: Vec<f32>,
    /// min base of those prisms
    pub rbase: Vec<f32>,
    /// bridge slab: base/top (NEG if none)
    pub bbase: Vec<f32>,
    pub btop: Vec<f32>,
    /// combined blocking surface: max(terrain, roof)
    pub surf: Vec<f32>,
    pub prisms: Vec<Prism>,
    pub trees: Vec<Tree>,
}

fn pip(px: f64, py: f64, poly: &[[f64; 2]]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (poly[i][0], poly[i][1]);
        let (xj, yj) = (poly[j][0], poly[j][1]);
        if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn flat_to_pts(v: &json::Value) -> Vec<[f64; 2]> {
    let a = v.arr();
    let mut out = Vec::with_capacity(a.len() / 2);
    let mut i = 0;
    while i + 1 < a.len() {
        out.push([a[i].f(), a[i + 1].f()]);
        i += 2;
    }
    out
}

impl Prism {
    pub fn contains_xy(&self, x: f64, y: f64) -> bool {
        if x < self.bbox[0] || x > self.bbox[1] || y < self.bbox[2] || y > self.bbox[3] {
            return false;
        }
        if !pip(x, y, &self.ring) {
            return false;
        }
        for h in &self.holes {
            if pip(x, y, h) {
                return false;
            }
        }
        true
    }
}

impl World {
    pub fn load(dir: &str) -> Result<World, String> {
        let wj = std::fs::read_to_string(format!("{}/v2/world.json", dir)).map_err(|e| format!("world.json: {}", e))?;
        let w = json::parse(&wj)?;
        let t = w.get("terrain");
        let nx = t.get("nx").f() as usize;
        let ny = t.get("ny").f() as usize;
        let min_x = t.get("minX").f();
        let min_y = t.get("minY").f();
        let scale = t.get("scale").f_or(0.01);
        let raw = std::fs::read(format!("{}/v2/terrain.i16", dir)).map_err(|e| format!("terrain: {}", e))?;
        let mut terrain = vec![0f32; nx * ny];
        for k in 0..(nx * ny).min(raw.len() / 2) {
            let v = i16::from_le_bytes([raw[2 * k], raw[2 * k + 1]]);
            terrain[k] = (v as f64 * scale) as f32;
        }
        let b = w.get("bounds");
        let bounds = [b.get("minX").f(), b.get("maxX").f(), b.get("minY").f(), b.get("maxY").f()];
        let mut prisms = Vec::new();
        for p in w.get("prisms").arr() {
            let ring = flat_to_pts(p.get("ring"));
            if ring.len() < 3 {
                continue;
            }
            let holes: Vec<Vec<[f64; 2]>> = p.get("holes").arr().iter().map(flat_to_pts).collect();
            let mut bb = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
            for q in &ring {
                bb[0] = bb[0].min(q[0]);
                bb[1] = bb[1].max(q[0]);
                bb[2] = bb[2].min(q[1]);
                bb[3] = bb[3].max(q[1]);
            }
            prisms.push(Prism {
                kind: match p.get("kind").s() {
                    "bridge" => 2,
                    "stadium" => 1,
                    _ => 0,
                },
                base: p.get("base").f(),
                top: p.get("top").f(),
                ring,
                holes,
                bbox: bb,
            });
        }
        let tr = w.get("trees");
        let fields: Vec<String> = tr.get("fields").arr().iter().map(|f| f.s().to_string()).collect();
        let nf = fields.len().max(1);
        let fi = |name: &str| fields.iter().position(|f| f == name);
        let data = tr.get("data").arr();
        let mut trees = Vec::new();
        let (ix, iy, iz, ih, icr, icb, itr, icx, icy) = (
            fi("x"),
            fi("y"),
            fi("z"),
            fi("height"),
            fi("crownR"),
            fi("crownBase"),
            fi("trunkR"),
            fi("crownX"),
            fi("crownY"),
        );
        let gf = |row: usize, i: Option<usize>, d: f64| -> f64 {
            match i {
                Some(i) => data.get(row * nf + i).map(|v| v.f_or(d)).unwrap_or(d),
                None => d,
            }
        };
        for r in 0..data.len() / nf {
            let x = gf(r, ix, 0.0);
            let y = gf(r, iy, 0.0);
            trees.push(Tree {
                x,
                y,
                z: gf(r, iz, 0.0),
                height: gf(r, ih, 8.0),
                crown_r: gf(r, icr, 3.0),
                crown_base: gf(r, icb, 3.0),
                trunk_r: gf(r, itr, 0.3),
                cx: gf(r, icx, x),
                cy: gf(r, icy, y),
            });
        }
        let n = nx * ny;
        let mut roof = vec![NEG; n];
        let mut rbase = vec![1.0e9f32; n];
        let mut bbase = vec![NEG; n];
        let mut btop = vec![NEG; n];
        for p in &prisms {
            let i0 = ((p.bbox[0] - min_x).floor().max(0.0)) as usize;
            let i1 = ((p.bbox[1] - min_x).ceil().max(0.0) as usize).min(nx - 1);
            let j0 = ((p.bbox[2] - min_y).floor().max(0.0)) as usize;
            let j1 = ((p.bbox[3] - min_y).ceil().max(0.0) as usize).min(ny - 1);
            for j in j0..=j1 {
                let y = min_y + j as f64;
                for i in i0..=i1 {
                    let x = min_x + i as f64;
                    if p.contains_xy(x, y) {
                        let k = j * nx + i;
                        if p.kind == 2 {
                            btop[k] = btop[k].max(p.top as f32);
                            if bbase[k] == NEG || (p.base as f32) < bbase[k] {
                                bbase[k] = p.base as f32;
                            }
                        } else {
                            roof[k] = roof[k].max(p.top as f32);
                            rbase[k] = rbase[k].min(p.base as f32);
                        }
                    }
                }
            }
        }
        let mut surf = vec![0f32; n];
        for k in 0..n {
            surf[k] = terrain[k].max(roof[k]);
        }
        Ok(World { nx, ny, min_x, min_y, bounds, terrain, roof, rbase, bbase, btop, surf, prisms, trees })
    }

    #[inline]
    pub fn cell(&self, x: f64, y: f64) -> Option<usize> {
        let i = (x - self.min_x).round();
        let j = (y - self.min_y).round();
        if i < 0.0 || j < 0.0 {
            return None;
        }
        let (i, j) = (i as usize, j as usize);
        if i >= self.nx || j >= self.ny {
            return None;
        }
        Some(j * self.nx + i)
    }

    /// Bilinear terrain height.
    pub fn ground(&self, x: f64, y: f64) -> f64 {
        let fx = (x - self.min_x).clamp(0.0, (self.nx - 1) as f64 - 1e-6);
        let fy = (y - self.min_y).clamp(0.0, (self.ny - 1) as f64 - 1e-6);
        let i = fx.floor() as usize;
        let j = fy.floor() as usize;
        let tx = fx - i as f64;
        let ty = fy - j as f64;
        let i1 = (i + 1).min(self.nx - 1);
        let j1 = (j + 1).min(self.ny - 1);
        let h00 = self.terrain[j * self.nx + i] as f64;
        let h10 = self.terrain[j * self.nx + i1] as f64;
        let h01 = self.terrain[j1 * self.nx + i] as f64;
        let h11 = self.terrain[j1 * self.nx + i1] as f64;
        (h00 * (1.0 - tx) + h10 * tx) * (1.0 - ty) + (h01 * (1.0 - tx) + h11 * tx) * ty
    }

    /// Height of the top surface (terrain or roof) at a point.
    pub fn surface(&self, x: f64, y: f64) -> f64 {
        match self.cell(x, y) {
            Some(k) => {
                let r = self.roof[k] as f64;
                let g = self.ground(x, y);
                if r > g {
                    r
                } else {
                    g
                }
            }
            None => 1.0e6,
        }
    }

    pub fn roof_at(&self, x: f64, y: f64) -> f64 {
        match self.cell(x, y) {
            Some(k) => self.roof[k] as f64,
            None => NEG as f64,
        }
    }

    pub fn in_bounds(&self, x: f64, y: f64) -> bool {
        x >= self.bounds[0] && x <= self.bounds[1] && y >= self.bounds[2] && y <= self.bounds[3]
    }

    /// Is the point inside solid geometry (terrain, building, bridge)?
    #[inline]
    pub fn solid(&self, x: f64, y: f64, z: f64) -> bool {
        match self.cell(x, y) {
            Some(k) => {
                let zf = z as f32;
                if zf < self.terrain[k] {
                    return true;
                }
                if zf < self.roof[k] && zf >= self.rbase[k] {
                    return true;
                }
                if zf >= self.bbase[k] && zf < self.btop[k] {
                    return true;
                }
                false
            }
            None => true,
        }
    }

    /// Line of sight between two points (buildings, terrain, bridges). Trees ignored.
    pub fn los(&self, a: V3, b: V3) -> bool {
        self.ray_block(a, b).is_none()
    }

    /// First blocked fraction along a->b (None if clear). Step ~0.7 m.
    pub fn ray_block(&self, a: V3, b: V3) -> Option<f64> {
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt().max(d[2].abs());
        let n = ((len / 0.7).ceil() as usize).max(1);
        for s in 1..n {
            let t = s as f64 / n as f64;
            if self.solid(a[0] + d[0] * t, a[1] + d[1] * t, a[2] + d[2] * t) {
                return Some(t);
            }
        }
        None
    }

    /// Rough tree occlusion count along a horizontal-ish sight line (crowns/trunks).
    pub fn tree_block(&self, a: V3, b: V3) -> f64 {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let l2 = dx * dx + dy * dy;
        if l2 < 1e-6 {
            return 0.0;
        }
        let minx = a[0].min(b[0]) - 8.0;
        let maxx = a[0].max(b[0]) + 8.0;
        let miny = a[1].min(b[1]) - 8.0;
        let maxy = a[1].max(b[1]) + 8.0;
        let mut acc = 0.0;
        for t in &self.trees {
            if t.cx < minx || t.cx > maxx || t.cy < miny || t.cy > maxy {
                continue;
            }
            // closest point on the segment to the crown center
            let u = (((t.cx - a[0]) * dx + (t.cy - a[1]) * dy) / l2).clamp(0.0, 1.0);
            let px = a[0] + dx * u - t.cx;
            let py = a[1] + dy * u - t.cy;
            let dist = (px * px + py * py).sqrt();
            if dist < t.crown_r {
                let z = a[2] + (b[2] - a[2]) * u;
                if z > t.z + t.crown_base && z < t.z + t.height {
                    acc += 2.0 * (t.crown_r * t.crown_r - dist * dist).sqrt();
                }
            }
        }
        acc
    }
}
