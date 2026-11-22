//! Small scalar helpers shared by the procedural renderer, plus the `Vec3`
//! type used by the 3D pipeline.

use std::ops::{Add, Mul, Neg, Sub};

pub const TAU: f32 = std::f32::consts::TAU;

/// Minimal 3D vector. World space: +x right, +y up, +z toward the camera.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn normalized(self) -> Vec3 {
        let l = self.length();
        if l > 1e-6 {
            self * (1.0 / l)
        } else {
            Vec3::new(0.0, 0.0, 0.0)
        }
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

/// Wrap an angle to (-PI, PI].
pub fn wrap_angle(a: f32) -> f32 {
    let mut a = a % TAU;
    if a > std::f32::consts::PI {
        a -= TAU;
    } else if a <= -std::f32::consts::PI {
        a += TAU;
    }
    a
}

pub fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// Classic cubic smoothstep; returns 0 below `e0`, 1 above `e1`.
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp01((x - e0) / (e1 - e0));
    t * t * (3.0 - 2.0 * t)
}

/// Deterministic 2D integer hash in [0, 1). Used for gravel pebbles,
/// scale sparkle and bubble respawn positions so frames are reproducible.
pub fn hash2(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841);
    h ^= h >> 13;
    h = h.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 16;
    (h & 0x00ff_ffff) as f32 / 0x0100_0000 as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothstep_endpoints() {
        assert_eq!(smoothstep(0.0, 1.0, -0.5), 0.0);
        assert_eq!(smoothstep(0.0, 1.0, 1.5), 1.0);
        assert_eq!(smoothstep(0.0, 1.0, 0.5), 0.5);
    }

    #[test]
    fn smoothstep_monotonic() {
        let mut prev = 0.0;
        for i in 0..=100 {
            let v = smoothstep(0.0, 10.0, i as f32 / 10.0);
            assert!(v >= prev);
            prev = v;
        }
    }

    #[test]
    fn vec3_ops() {
        let a = Vec3::new(1.0, 0.0, 0.0);
        let b = Vec3::new(0.0, 1.0, 0.0);
        assert_eq!(a.cross(b), Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(a.dot(b), 0.0);
        assert_eq!((a + b * 2.0).length(), 5.0f32.sqrt());
        assert_eq!((-a).x, -1.0);
        let n = Vec3::new(3.0, 0.0, 0.0).normalized();
        assert!((n.x - 1.0).abs() < 1e-6);
        assert_eq!(Vec3::default().normalized(), Vec3::default());
    }

    #[test]
    fn wrap_angle_range() {
        use std::f32::consts::PI;
        assert!((wrap_angle(3.0 * PI).abs() - PI).abs() < 1e-4);
        assert!((wrap_angle(-3.0 * PI).abs() - PI).abs() < 1e-4);
        assert_eq!(wrap_angle(0.5), 0.5);
        for i in -100..100 {
            let a = wrap_angle(i as f32 * 0.37);
            assert!(a > -PI && a <= PI);
        }
    }

    #[test]
    fn hash_is_deterministic_and_in_range() {
        assert_eq!(hash2(7, -3), hash2(7, -3));
        for i in -50..50 {
            let v = hash2(i, i * 3 + 1);
            assert!((0.0..1.0).contains(&v));
        }
    }
}
