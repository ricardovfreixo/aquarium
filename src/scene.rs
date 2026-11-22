//! The aquarium environment in the same ASCII language as the fish: a bare
//! black backdrop with a sparse dotted floor, two swaying plant strands made
//! of strokes, a few drifting motes, and slowly rising `o`-glyph bubbles.
//! Everything lives in the same world space and goes through the same
//! perspective camera and depth buffer as the fish, so parallax and
//! occlusion come for free. All placement is hash-based and deterministic.

use crate::camera::{Camera, TANK_X, TANK_Z};
use crate::cells::{Color, GlyphBuffer};
use crate::math::{clamp01, hash2, Vec3, TAU};

const FLOOR_Y: f32 = -0.92;
const SURFACE_Y: f32 = 0.98;

const BUBBLE_TINT: Color = [150, 200, 228];
const PLANT_DARK: Color = [26, 72, 44];
const PLANT_LIT: Color = [52, 128, 72];
const PEBBLE: Color = [64, 58, 54];
const MOTE: Color = [42, 58, 78];

pub struct Scene {
    bubbles: Vec<Bubble>,
    respawn_counter: u32,
}

struct Bubble {
    pos: Vec3,
    r: f32,
    speed: f32,
    wobble: f32,
}

impl Scene {
    pub fn new() -> Self {
        // A sparse column of bubbles, staggered in height and phase.
        let bubbles = (0..7)
            .map(|i| {
                let f = i as f32;
                Bubble {
                    pos: Vec3::new(
                        -0.9 + 0.35 * f + 0.15 * hash2(i, 7),
                        FLOOR_Y + 0.25 * f,
                        -0.3 + 0.55 * hash2(i, 13),
                    ),
                    r: 0.022 + 0.020 * hash2(i, 3),
                    speed: 0.16 + 0.10 * hash2(i, 11),
                    wobble: hash2(i, 23) * TAU,
                }
            })
            .collect();
        Self {
            bubbles,
            respawn_counter: 0,
        }
    }

    pub fn update(&mut self, dt: f32) {
        for b in &mut self.bubbles {
            b.pos.y += b.speed * dt;
            b.wobble += dt * 2.4;
            b.pos.x += b.wobble.sin() * 0.05 * dt;
            if b.pos.y > SURFACE_Y {
                // Pop; respawn near the floor at a hashed position.
                self.respawn_counter += 1;
                let n = self.respawn_counter as i32;
                b.pos.x = -TANK_X * 0.8 + 1.6 * TANK_X * hash2(n, 91);
                b.pos.z = -TANK_Z * 0.7 + 1.4 * TANK_Z * hash2(n, 37);
                b.pos.y = FLOOR_Y + 0.02;
            }
        }
    }

    /// Paint the environment and the bubbles into the glyph buffer.
    pub fn paint(&self, buf: &mut GlyphBuffer, cam: &Camera, t: f32) {
        self.paint_floor(buf, cam);
        self.paint_motes(buf, cam, t);
        self.paint_plants(buf, cam, t);
        self.paint_bubbles(buf, cam);
    }

    fn plot(&self, buf: &mut GlyphBuffer, cam: &Camera, p: Vec3, ch: char, c: Color) {
        if let Some(pr) = cam.project(p) {
            buf.plot(pr.x, pr.y, pr.depth, ch, c);
        }
    }

    /// Sparse hashed pebbles across the tank floor.
    fn paint_floor(&self, buf: &mut GlyphBuffer, cam: &Camera) {
        let nx = 52;
        let nz = 20;
        for ix in 0..nx {
            for iz in 0..nz {
                let h = hash2(ix, iz * 7 + 1);
                if h < 0.62 {
                    continue; // sparse: most of the floor stays black
                }
                let x = -TANK_X + 2.0 * TANK_X * ix as f32 / (nx - 1) as f32;
                let z = -TANK_Z + 2.0 * TANK_Z * iz as f32 / (nz - 1) as f32;
                let y = FLOOR_Y + 0.03 * hash2(ix * 3 + 5, iz);
                let ch = if h > 0.9 { ':' } else { '.' };
                let shade = 0.55 + 0.75 * hash2(ix + 31, iz + 17);
                self.plot(buf, cam, Vec3::new(x, y, z), ch, scale(PEBBLE, shade));
            }
        }
    }

    /// A few dozen slowly drifting motes so the water has depth cues
    /// without losing the sparse black look.
    fn paint_motes(&self, buf: &mut GlyphBuffer, cam: &Camera, t: f32) {
        for i in 0..34 {
            let hx = hash2(i, 101);
            let hy = hash2(i, 103);
            let hz = hash2(i, 107);
            // Slow drift upward and sideways, wrapped into the tank.
            let x = -TANK_X + 2.0 * TANK_X * (hx + 0.006 * t * (0.3 + hx)).rem_euclid(1.0);
            let y = FLOOR_Y + 1.9 * (hy + 0.004 * t).rem_euclid(1.0);
            let z = -TANK_Z + 2.0 * TANK_Z * hz;
            let tw = 0.55 + 0.45 * (t * 0.8 + i as f32).sin();
            let ch = if hash2(i, 109) > 0.8 { '+' } else { '.' };
            self.plot(buf, cam, Vec3::new(x, y, z), ch, scale(MOTE, tw));
        }
    }

    /// Two seaweed strands near the back corners, built from stroke glyphs.
    fn paint_plants(&self, buf: &mut GlyphBuffer, cam: &Camera, t: f32) {
        for (i, &(bx, bz, hgt)) in [(-1.75f32, -0.55f32, 1.15f32), (1.85, -0.35, 0.95)]
            .iter()
            .enumerate()
        {
            let nodes = 26;
            for n in 0..nodes {
                let f = n as f32 / (nodes - 1) as f32; // 0 root, 1 tip
                let sway = (t * 0.55 + f * 3.1 + i as f32 * 2.3).sin() * 0.16 * f.powf(1.4);
                let lean = 0.10 * f * if i == 0 { 1.0 } else { -1.0 };
                let p = Vec3::new(bx + sway + lean, FLOOR_Y + hgt * f, bz);
                // Stroke direction follows the local tangent of the strand.
                let tangent = Vec3::new(
                    0.16 * f.powf(1.4) * 3.1 * (t * 0.55 + f * 3.1 + i as f32 * 2.3).cos() + lean,
                    hgt,
                    0.0,
                );
                let ch = strand_glyph(cam, p, tangent);
                let c = mix(PLANT_DARK, PLANT_LIT, 0.25 + 0.75 * f);
                self.plot(buf, cam, p, ch, c);
            }
        }
    }

    /// Bubbles: small bright `o`/`O` rings drifting upward.
    fn paint_bubbles(&self, buf: &mut GlyphBuffer, cam: &Camera) {
        for b in &self.bubbles {
            if let Some(pr) = cam.project(b.pos) {
                // Apparent radius in cells picks the glyph weight.
                let apparent = b.r * 140.0 / pr.depth;
                let ch = if apparent < 0.9 {
                    '.'
                } else if apparent < 1.9 {
                    'o'
                } else {
                    'O'
                };
                let lift = 0.55 + 0.45 * clamp01((b.pos.y - FLOOR_Y) / 1.6);
                buf.plot(pr.x, pr.y, pr.depth, ch, scale(BUBBLE_TINT, lift));
            }
        }
    }
}

/// Pick a stroke glyph matching the projected tangent of a plant strand.
fn strand_glyph(cam: &Camera, p: Vec3, tangent: Vec3) -> char {
    let a = cam.project(p);
    let b = cam.project(p + tangent * 0.05);
    if let (Some(a), Some(b)) = (a, b) {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        if dx.abs() + dy.abs() < 0.05 {
            return '.';
        }
        let deg = dy.atan2(dx).to_degrees();
        if deg.abs() < 25.0 {
            '~'
        } else if deg.abs() > 65.0 {
            '|'
        } else if deg > 0.0 {
            '\\'
        } else {
            '/'
        }
    } else {
        '|'
    }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = clamp01(t);
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
    ]
}

fn scale(c: Color, s: f32) -> Color {
    [
        (c[0] as f32 * s).clamp(0.0, 255.0) as u8,
        (c[1] as f32 * s).clamp(0.0, 255.0) as u8,
        (c[2] as f32 * s).clamp(0.0, 255.0) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubbles_rise_and_respawn_at_floor() {
        let mut scene = Scene::new();
        let y0 = scene.bubbles[0].pos.y;
        scene.update(0.1);
        assert!(scene.bubbles[0].pos.y > y0, "bubbles should rise");

        scene.bubbles[0].pos.y = SURFACE_Y + 0.01;
        scene.update(0.016);
        assert!(
            (scene.bubbles[0].pos.y - FLOOR_Y).abs() < 0.1,
            "popped bubble should respawn near the floor"
        );
    }

    #[test]
    fn frame_is_deterministic_and_sparse() {
        let cam = Camera::new(120, 36);
        let scene = Scene::new();
        let mut a = GlyphBuffer::new(120, 36);
        let mut b = GlyphBuffer::new(120, 36);
        scene.paint(&mut a, &cam, 1.25);
        scene.paint(&mut b, &cam, 1.25);
        assert_eq!(a.to_text(), b.to_text());
        // The backdrop stays mostly black; decoration is deliberately sparse.
        let fill = a.nonblank() as f32 / (120.0 * 36.0);
        assert!(fill < 0.25, "scene alone should be sparse, fill={fill}");
        assert!(a.nonblank() > 40, "but not empty");
    }

    #[test]
    fn floor_sits_below_plants_and_bubbles_use_o_glyphs() {
        let cam = Camera::new(120, 36);
        let mut scene = Scene::new();
        // Put one bubble right in front of the camera so it renders large.
        scene.bubbles[0].pos = Vec3::new(0.0, 0.0, 0.9);
        scene.bubbles[0].r = 0.05;
        let mut buf = GlyphBuffer::new(120, 36);
        scene.paint(&mut buf, &cam, 0.0);
        let text = buf.to_text();
        assert!(text.contains('o') || text.contains('O'));
        // Floor dots occupy the lower part of the frame.
        let mut floor_rows = 0;
        for (y, line) in text.lines().enumerate() {
            if line.contains('.') || line.contains(':') {
                floor_rows = floor_rows.max(y);
            }
        }
        assert!(floor_rows > 36 / 2, "floor should be in the lower half");
    }
}
