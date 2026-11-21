//! 2 m navigation grid and A* path planning over walkable surfaces (ground and roofs).

use crate::world::World;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

pub const CS: f64 = 2.0;

pub struct Nav {
    pub w: usize,
    pub h: usize,
    pub ox: f64,
    pub oy: f64,
    /// surface height at the cell center
    pub hgt: Vec<f32>,
    /// standable: no wall within the mech radius
    pub stand: Vec<bool>,
    /// open-sky ground cell (not a roof)
    pub ground: Vec<bool>,
    // A* scratch
    g: Vec<f32>,
    from: Vec<u32>,
    gen: Vec<u32>,
    closed: Vec<u32>,
    cur_gen: u32,
}

#[derive(Copy, Clone, PartialEq)]
struct Node {
    f: f32,
    i: u32,
}
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.partial_cmp(&self.f).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

const DIRS: [(i32, i32, f32); 8] = [
    (1, 0, 1.0),
    (-1, 0, 1.0),
    (0, 1, 1.0),
    (0, -1, 1.0),
    (1, 1, 1.41421),
    (1, -1, 1.41421),
    (-1, 1, 1.41421),
    (-1, -1, 1.41421),
];

impl Nav {
    pub fn build(world: &World, margin: f64) -> Nav {
        let ox = world.bounds[0];
        let oy = world.bounds[2];
        let w = ((world.bounds[1] - ox) / CS).floor() as usize + 1;
        let h = ((world.bounds[3] - oy) / CS).floor() as usize + 1;
        let n = w * h;
        let mut hgt = vec![0f32; n];
        let mut ground = vec![false; n];
        for j in 0..h {
            for i in 0..w {
                let x = ox + i as f64 * CS;
                let y = oy + j as f64 * CS;
                let s = world.surface(x, y);
                hgt[j * w + i] = s as f32;
                ground[j * w + i] = world.roof_at(x, y) < world.ground(x, y) + 0.5;
            }
        }
        // standable: within radius 3 m, no surface sample higher than own + 1.3 (a wall)
        let mut stand = vec![false; n];
        let r = 3.0f64;
        let offs: Vec<(f64, f64)> = {
            let mut v = Vec::new();
            let mut dy = -r;
            while dy <= r + 1e-9 {
                let mut dx = -r;
                while dx <= r + 1e-9 {
                    if dx * dx + dy * dy <= r * r + 1e-9 {
                        v.push((dx, dy));
                    }
                    dx += 1.0;
                }
                dy += 1.0;
            }
            v
        };
        for j in 0..h {
            for i in 0..w {
                let x = ox + i as f64 * CS;
                let y = oy + j as f64 * CS;
                if x < world.bounds[0] + margin
                    || x > world.bounds[1] - margin
                    || y < world.bounds[2] + margin
                    || y > world.bounds[3] - margin
                {
                    continue;
                }
                let me = hgt[j * w + i] as f64;
                let mut ok = true;
                for (dx, dy) in &offs {
                    let s = world.surface(x + dx, y + dy);
                    if s > me + 1.3 {
                        ok = false;
                        break;
                    }
                    // bridges at head height block standing
                    if let Some(k) = world.cell(x + dx, y + dy) {
                        let bb = world.bbase[k] as f64;
                        if bb > -1.0e8 && bb > me + 0.5 && bb < me + 9.5 {
                            ok = false;
                            break;
                        }
                    }
                }
                stand[j * w + i] = ok;
            }
        }
        Nav {
            w,
            h,
            ox,
            oy,
            hgt,
            stand,
            ground,
            g: vec![0.0; n],
            from: vec![0; n],
            gen: vec![0; n],
            closed: vec![0; n],
            cur_gen: 0,
        }
    }

    #[inline]
    pub fn idx(&self, x: f64, y: f64) -> Option<usize> {
        let i = ((x - self.ox) / CS).round();
        let j = ((y - self.oy) / CS).round();
        if i < 0.0 || j < 0.0 || i >= self.w as f64 || j >= self.h as f64 {
            return None;
        }
        Some(j as usize * self.w + i as usize)
    }
    #[inline]
    pub fn center(&self, k: usize) -> (f64, f64) {
        let i = k % self.w;
        let j = k / self.w;
        (self.ox + i as f64 * CS, self.oy + j as f64 * CS)
    }

    /// Can you walk from cell a to adjacent cell b?
    #[inline]
    pub fn step_ok(&self, a: usize, b: usize, dist: f32) -> bool {
        if !self.stand[b] {
            return false;
        }
        let dh = self.hgt[b] - self.hgt[a];
        dh <= 0.6 + 1.05 * dist * CS as f32
    }

    /// Nearest standable cell to (x,y) within `rad` cells, preferring similar height.
    pub fn nearest_stand(&self, x: f64, y: f64, z: Option<f64>, rad: i32) -> Option<usize> {
        let ci = ((x - self.ox) / CS).round() as i32;
        let cj = ((y - self.oy) / CS).round() as i32;
        let mut best: Option<(f64, usize)> = None;
        for dj in -rad..=rad {
            for di in -rad..=rad {
                let i = ci + di;
                let j = cj + dj;
                if i < 0 || j < 0 || i >= self.w as i32 || j >= self.h as i32 {
                    continue;
                }
                let k = j as usize * self.w + i as usize;
                if !self.stand[k] {
                    continue;
                }
                let (cx, cy) = self.center(k);
                let mut d = (cx - x).powi(2) + (cy - y).powi(2);
                if let Some(z) = z {
                    let dz = self.hgt[k] as f64 - z;
                    if dz > 2.0 {
                        d += dz * dz * 4.0;
                    }
                    if dz < -3.0 {
                        d += dz * dz * 0.5;
                    }
                }
                if best.map_or(true, |b| d < b.0) {
                    best = Some((d, k));
                }
            }
        }
        best.map(|b| b.1)
    }

    /// A* from (sx,sy,sz) to (gx,gy). Returns waypoints (cell centers) excluding the start.
    /// If the goal is unreachable, returns the path to the explored cell closest to the goal.
    /// `cost` adds extra per-cell cost (e.g. danger), multiplied by step length.
    pub fn path(
        &mut self,
        sx: f64,
        sy: f64,
        sz: f64,
        gx: f64,
        gy: f64,
        max_expand: usize,
        cost: &dyn Fn(usize) -> f32,
    ) -> Option<Vec<(f64, f64)>> {
        let s = self.nearest_stand(sx, sy, Some(sz), 4)?;
        let gidx = self.nearest_stand(gx, gy, None, 6);
        self.cur_gen = self.cur_gen.wrapping_add(1);
        if self.cur_gen == 0 {
            self.gen.iter_mut().for_each(|g| *g = 0);
            self.closed.iter_mut().for_each(|g| *g = 0);
            self.cur_gen = 1;
        }
        let gen = self.cur_gen;
        let (gcx, gcy) = match gidx {
            Some(k) => self.center(k),
            None => (gx, gy),
        };
        let hfun = |k: usize, w: usize| -> f32 {
            let i = (k % w) as f64;
            let j = (k / w) as f64;
            let dx = ((i * CS + self.ox) - gcx).abs() / CS;
            let dy = ((j * CS + self.oy) - gcy).abs() / CS;
            let (a, b) = if dx > dy { (dx, dy) } else { (dy, dx) };
            (a + 0.41421 * b) as f32
        };
        let mut heap = BinaryHeap::new();
        self.g[s] = 0.0;
        self.gen[s] = gen;
        self.from[s] = s as u32;
        heap.push(Node { f: hfun(s, self.w), i: s as u32 });
        let mut best = (hfun(s, self.w), s);
        let mut expanded = 0;
        let mut found = None;
        while let Some(Node { i, .. }) = heap.pop() {
            let k = i as usize;
            if self.closed[k] == gen {
                continue;
            }
            self.closed[k] = gen;
            let hk = hfun(k, self.w);
            if hk < best.0 {
                best = (hk, k);
            }
            if Some(k) == gidx {
                found = Some(k);
                break;
            }
            expanded += 1;
            if expanded > max_expand {
                break;
            }
            let ci = (k % self.w) as i32;
            let cj = (k / self.w) as i32;
            let gk = self.g[k];
            for &(di, dj, dl) in DIRS.iter() {
                let ni = ci + di;
                let nj = cj + dj;
                if ni < 0 || nj < 0 || ni >= self.w as i32 || nj >= self.h as i32 {
                    continue;
                }
                let nk = nj as usize * self.w + ni as usize;
                if self.closed[nk] == gen {
                    continue;
                }
                if !self.step_ok(k, nk, dl) {
                    continue;
                }
                if di != 0 && dj != 0 {
                    // no corner cutting
                    let a = cj as usize * self.w + ni as usize;
                    let b = nj as usize * self.w + ci as usize;
                    if !self.stand[a] || !self.stand[b] {
                        continue;
                    }
                }
                let ng = gk + dl * (1.0 + cost(nk));
                if self.gen[nk] != gen || ng < self.g[nk] {
                    self.gen[nk] = gen;
                    self.g[nk] = ng;
                    self.from[nk] = k as u32;
                    heap.push(Node { f: ng + hfun(nk, self.w), i: nk as u32 });
                }
            }
        }
        let end = found.unwrap_or(best.1);
        let mut cells = Vec::new();
        let mut k = end;
        let mut guard = 0;
        while k != s && guard < 100000 {
            cells.push(k);
            k = self.from[k] as usize;
            guard += 1;
        }
        cells.reverse();
        Some(cells.into_iter().map(|k| self.center(k)).collect())
    }

    /// Straight walkable segment check between two points on the nav grid.
    pub fn walkable_line(&self, ax: f64, ay: f64, bx: f64, by: f64) -> bool {
        let d = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
        let n = (d / (CS * 0.5)).ceil() as usize;
        let mut prev = match self.idx(ax, ay) {
            Some(k) => k,
            None => return false,
        };
        for s in 1..=n {
            let t = s as f64 / n as f64;
            let k = match self.idx(ax + (bx - ax) * t, ay + (by - ay) * t) {
                Some(k) => k,
                None => return false,
            };
            if k != prev {
                if !self.step_ok(prev, k, 1.41) {
                    return false;
                }
                prev = k;
            }
        }
        true
    }
}
