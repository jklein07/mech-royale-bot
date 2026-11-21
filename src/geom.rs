//! Small geometry helpers.

use crate::proto::V3;
use std::f64::consts::PI;

#[inline]
pub fn wrap(a: f64) -> f64 {
    let mut a = a % (2.0 * PI);
    if a > PI {
        a -= 2.0 * PI;
    }
    if a < -PI {
        a += 2.0 * PI;
    }
    a
}
#[inline]
pub fn d2(a: &[f64], b: &[f64]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}
#[inline]
pub fn d3(a: &V3, b: &V3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
#[inline]
pub fn yaw_to(a: &[f64], b: &[f64]) -> f64 {
    (b[1] - a[1]).atan2(b[0] - a[0])
}
#[inline]
pub fn norm2(x: f64, y: f64) -> (f64, f64) {
    let l = (x * x + y * y).sqrt();
    if l < 1e-9 {
        (0.0, 0.0)
    } else {
        (x / l, y / l)
    }
}
#[inline]
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
#[inline]
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
pub fn mul(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
#[inline]
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
pub fn len(a: V3) -> f64 {
    dot(a, a).sqrt()
}
pub fn dir(yaw: f64, pitch: f64) -> V3 {
    [pitch.cos() * yaw.cos(), pitch.cos() * yaw.sin(), pitch.sin()]
}
/// yaw, pitch of vector
pub fn angles(v: V3) -> (f64, f64) {
    let h = (v[0] * v[0] + v[1] * v[1]).sqrt();
    (v[1].atan2(v[0]), v[2].atan2(h))
}

/// Intercept time for a projectile with speed `s` from `m` to a target at `p` moving with `v`.
pub fn intercept(m: V3, p: V3, v: V3, s: f64) -> Option<f64> {
    let r = sub(p, m);
    let a = dot(v, v) - s * s;
    let b = 2.0 * dot(r, v);
    let c = dot(r, r);
    if a.abs() < 1e-9 {
        if b.abs() < 1e-9 {
            return None;
        }
        let t = -c / b;
        return if t > 0.0 { Some(t) } else { None };
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let t1 = (-b - sq) / (2.0 * a);
    let t2 = (-b + sq) / (2.0 * a);
    let t = if t1 > 0.0 && t2 > 0.0 {
        t1.min(t2)
    } else if t1 > 0.0 {
        t1
    } else if t2 > 0.0 {
        t2
    } else {
        return None;
    };
    Some(t)
}

/// Pitch to hit a point at horizontal distance `dx` and height `dz` with speed `v`, gravity `g` (low arc).
pub fn ballistic_pitch(dx: f64, dz: f64, v: f64, g: f64, high: bool) -> Option<f64> {
    let v2 = v * v;
    let disc = v2 * v2 - g * (g * dx * dx + 2.0 * dz * v2);
    if disc < 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let th = if high { ((v2 + sq) / (g * dx)).atan() } else { ((v2 - sq) / (g * dx)).atan() };
    Some(th)
}

pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.f()
    }
    pub fn gauss(&mut self) -> f64 {
        let u1 = self.f().max(1e-12);
        let u2 = self.f();
        (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
    }
}
