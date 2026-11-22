//! The betta fish: a procedural 3D model sampled into colored glyphs.
//!
//! The fish is built in a local coordinate system (+x toward the head, +y
//! up, +z to the fish's left) from parametric parts — a deep-bodied
//! ellipsoid body, two eyes, a broad ruffled tail fan, flowing dorsal and
//! anal fins, two ventral filaments and two fluttering pectoral fins. Each
//! frame the parts are sampled on regular grids, deformed by a traveling
//! swimming wave, rotated by the fish's yaw/pitch/roll, and projected
//! through the perspective camera into the glyph buffer with a per-cell
//! z-test — so the fish foreshortens, turns and occludes itself like a real
//! 3D object instead of mirroring a 2D sprite.
//!
//! Glyph choice encodes shading: the body uses an ink-density ramp
//! ('.' up to '$'), fin membranes use directional strokes ('-', '|', '/',
//! '\\') aligned with their flow, and grazing body silhouettes get '(' / ')'.

use crate::camera::Camera;
use crate::cells::{Color, GlyphBuffer};
use crate::math::{clamp01, hash2, smoothstep, wrap_angle, Vec3, TAU};

const DEEP_BLUE: Color = [42, 82, 205];
const ROYAL: Color = [72, 145, 255];
const CYAN: Color = [125, 230, 255];
const FIN_RED: Color = [238, 62, 100];
const FIN_DARK: Color = [168, 34, 82];
const EYE_DARK: Color = [12, 12, 20];

/// Ink-density ramp for shaded body cells, lightest to heaviest.
const BODY_RAMP: &[char] = &[
    '.', ':', 'o', 'O', '0', 'Q', 'B', '8', '&', '%', '#', '@', '$',
];

/// Overall size of the fish in world units (local units are body lengths).
const SCALE: f32 = 1.25;

/// Patrol route through the tank (x, y, z); the fish steers from mark to
/// mark, banking through the turns. All inside the tank bounds.
const WAYPOINTS: &[Vec3] = &[
    Vec3::new(1.30, 0.05, -0.45),
    Vec3::new(1.40, -0.15, 0.35),
    Vec3::new(0.45, 0.20, 0.62),
    Vec3::new(-0.75, 0.28, 0.52),
    Vec3::new(-1.40, 0.02, -0.05),
    Vec3::new(-1.10, -0.22, -0.58),
    Vec3::new(0.15, -0.02, -0.68),
];

pub struct Betta {
    pub pos: Vec3,
    /// Heading in the water plane; 0 faces +x (broadside to the camera).
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    /// Current patrol mark.
    wp: usize,
    /// Tail-beat phase in radians; drives the swim wave and fin ripples.
    phase: f32,
    /// Slow clock for vertical bobbing.
    wander: f32,
    /// Current speed in world units/sec (exposed for tests/HUD).
    speed: f32,
}

impl Betta {
    pub fn new() -> Self {
        Self {
            pos: Vec3::new(0.6, 0.0, 0.15),
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            wp: 0,
            phase: 0.0,
            wander: 0.0,
            speed: 0.3,
        }
    }

    /// Advance the swim simulation: steer along the patrol route, bank into
    /// turns, bob gently, and beat the tail faster when working harder.
    pub fn update(&mut self, dt: f32) {
        self.wander += dt;

        // Steer toward the current mark; advance when close enough.
        let target = WAYPOINTS[self.wp];
        let to = target - self.pos;
        let flat = (to.x * to.x + to.z * to.z).sqrt();
        if flat < 0.45 {
            self.wp = (self.wp + 1) % WAYPOINTS.len();
        }
        let desired_yaw = (-to.z).atan2(to.x);
        let dyaw = wrap_angle(desired_yaw - self.yaw);
        let yaw_rate = (dyaw * 1.8).clamp(-1.1, 1.1);
        self.yaw = wrap_angle(self.yaw + yaw_rate * dt);

        // Bank into the turn; ease toward the bank angle.
        let roll_t = (yaw_rate * 0.55).clamp(-0.5, 0.5);
        self.roll += (roll_t - self.roll) * (2.5 * dt).min(1.0);

        // Vertical: ease toward the mark's depth plus a slow bob, pitch
        // into the climb or dive.
        let ty = target.y + 0.10 * (self.wander * 0.23).sin();
        let vy = ((ty - self.pos.y) * 1.4).clamp(-0.30, 0.30);
        self.pos.y += vy * dt;
        let pitch_t = (vy * 1.3).clamp(-0.35, 0.35);
        self.pitch += (pitch_t - self.pitch) * (2.0 * dt).min(1.0);

        // Slower while turning hard.
        let effort = 1.0 - 0.55 * yaw_rate.abs();
        self.speed = 0.30 * effort + 0.06;
        self.pos.x += self.yaw.cos() * self.speed * dt;
        self.pos.z += -self.yaw.sin() * self.speed * dt;

        // Safety clamp; the route should never reach the walls.
        self.pos.x = self.pos.x.clamp(-1.7, 1.7);
        self.pos.y = self.pos.y.clamp(-0.6, 0.65);
        self.pos.z = self.pos.z.clamp(-0.85, 0.85);

        // Tail beat: ~1 Hz while cruising, faster when pushing hard.
        self.phase = (self.phase + dt * TAU * (0.55 + 1.1 * self.speed)) % TAU;
    }

    // ----- local-space shape -------------------------------------------------

    /// Body half-height at axial coordinate u (0 = peduncle, 1 = snout).
    /// Bettas are deep-bodied, fuller toward the head.
    fn body_half_h(&self, u: f32) -> f32 {
        let s = clamp01((u + 0.10) / 1.18);
        let profile = (std::f32::consts::PI * s).sin().powf(0.9);
        let fullness = 1.0 - 0.22 * smoothstep(0.72, 1.0, u);
        0.30 * profile * fullness
    }

    /// Body half-width; bettas are laterally compressed.
    fn body_half_w(&self, u: f32) -> f32 {
        0.012 + 0.42 * self.body_half_h(u)
    }

    /// Axial coordinate: u = 0 at the peduncle (x = -0.45), 1 at the snout.
    fn u_to_x(u: f32) -> f32 {
        -0.45 + u
    }

    /// Lateral swimming wave, amplitude growing toward the tail.
    fn wave(&self, u: f32) -> f32 {
        let tailness = clamp01(1.0 - u).powf(1.6);
        0.085 * tailness * (3.0 * u - self.phase).sin()
    }

    /// Body surface point at axial u and cross-section angle th.
    fn body_point(&self, u: f32, th: f32) -> Vec3 {
        Vec3::new(
            Self::u_to_x(u),
            self.body_half_h(u) * th.cos(),
            self.body_half_w(u) * th.sin() + self.wave(u),
        )
    }

    /// Tail-fan membrane point. a in [-1, 1] sweeps the fan from the lower
    /// lobe to the upper; t in [0, 1] runs from the peduncle to the edge.
    fn tail_point(&self, a: f32, t: f32) -> Vec3 {
        let alpha = a * 1.05;
        let ruffle = 0.84 + 0.16 * (4.5 * a - 0.7 * self.phase.sin()).sin();
        let rmax = 0.74 * ruffle;
        let dir = Vec3::new(-alpha.cos(), alpha.sin(), 0.0);
        // Root spreads a little along the peduncle edge and sits slightly
        // inside the body, so fan and body connect with no dark gap.
        let mut p = Vec3::new(-0.40, 0.08 * a, self.wave(0.0) * 0.5) + dir * (rmax * t);
        // Lobes sweep backward and the lower lobe droops for the veil look.
        p.x -= 0.20 * t * t * a.abs();
        p.y -= 0.10 * t * t * (-a).max(0.0);
        // Follow the peduncle wave, with a slower membrane ripple on top.
        p.z += self.wave(0.0) * (1.0 - 0.5 * t) + 0.11 * t * (2.0 * a - self.phase).sin();
        p
    }

    /// Dorsal fin membrane. s in [0, 1] runs from front to rear along the
    /// back; t from the body to the edge.
    fn dorsal_point(&self, s: f32, t: f32) -> Vec3 {
        let u = 0.42 - 0.72 * s;
        let ruffle = 0.82 + 0.18 * (8.0 * s - 0.5 * self.phase.sin()).sin();
        let ext = 0.34 * (std::f32::consts::PI * s).sin().powf(0.75) * ruffle;
        let mut p = Vec3::new(
            u - 0.10 * ext * t - 0.30 * ext * t * t,
            self.body_half_h(u) * 0.96 + ext * t,
            self.wave(u) * 0.9 + 0.06 * t * (TAU * 1.4 * s - 0.9 * self.phase).sin(),
        );
        p.z += 0.0; // membrane stays near the body plane
        p
    }

    /// Anal fin membrane: long and flowing, trailing down and back.
    fn anal_point(&self, s: f32, t: f32) -> Vec3 {
        let u = 0.16 - 0.58 * s;
        let ruffle = 0.82 + 0.18 * (7.0 * s + 2.0 - 0.5 * self.phase.sin()).sin();
        let ext = 0.52 * (std::f32::consts::PI * s).sin().powf(0.85) * ruffle;
        Vec3::new(
            u - 0.12 * ext * t - 0.55 * ext * t * t,
            -self.body_half_h(u) * 0.96 - ext * t,
            self.wave(u) * 0.9 + 0.09 * t * (TAU * 1.8 * s + 1.1 - 0.9 * self.phase).sin(),
        )
    }

    /// Pectoral fin: a small fan on each flank that beats like an oar.
    fn pectoral_point(&self, side: f32, a: f32, t: f32) -> Vec3 {
        let hinge = Vec3::new(0.30, -0.02, side * (self.body_half_w(0.30) + 0.005));
        let lag = if side > 0.0 { 0.0 } else { 2.2 };
        let beat = (2.0 * self.phase + lag).sin();
        let open = 0.55 + 0.30 * beat;
        // Fan sweeps back and down, tilted just enough out of the body plane
        // to stay readable from the front; `open` pumps with the beat.
        let dir = Vec3::new(
            -0.60 - 0.30 * (1.0 - a.abs()),
            -0.60 * a,
            side * 0.55 * open,
        )
        .normalized();
        let r = 0.20 * (0.85 + 0.15 * (3.0 * a).sin());
        hinge + dir * (r * t)
    }

    /// Ventral filament: a thin red thread trailing under the head.
    fn filament_point(&self, side: f32, t: f32) -> Vec3 {
        Vec3::new(
            0.24 - 0.50 * t,
            -self.body_half_h(0.24) - 0.50 * t + 0.10 * t * t,
            side * 0.05 + 0.05 * t * (3.0 * t - self.phase).sin(),
        )
    }

    // ----- transforms --------------------------------------------------------

    /// Rotate a local vector into world orientation (no translation).
    fn rotate(&self, p: Vec3) -> Vec3 {
        let (sr, cr) = self.roll.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        // roll about the body axis (x)
        let (x1, y1, z1) = (p.x, p.y * cr - p.z * sr, p.y * sr + p.z * cr);
        // pitch about z
        let (x2, y2) = (x1 * cp - y1 * sp, x1 * sp + y1 * cp);
        // yaw about y
        Vec3::new(x2 * cy + z1 * sy, y2, -x2 * sy + z1 * cy)
    }

    fn to_world(&self, p: Vec3) -> Vec3 {
        self.pos + self.rotate(p * SCALE)
    }

    /// Pin the pose (used by `--pose` snapshots): hold the fish near the
    /// tank center at a fixed heading while fins keep undulating.
    pub fn pin(&mut self, yaw: f32) {
        self.pos = Vec3::new(0.3, 0.0, 0.0);
        self.yaw = yaw;
        self.pitch = 0.0;
        self.roll = 0.0;
    }

    // ----- painting ----------------------------------------------------------

    /// Project a local point and plot a glyph with a depth bias.
    fn plot(&self, buf: &mut GlyphBuffer, cam: &Camera, p: Vec3, bias: f32, ch: char, c: Color) {
        let w = self.to_world(p);
        if let Some(pr) = cam.project(w) {
            buf.plot(pr.x, pr.y, pr.depth + bias, ch, c);
        }
    }

    /// Pick the fin stroke glyph whose direction matches the membrane's
    /// projected grain. `p` and `grain` are local-space.
    fn stroke(&self, cam: &Camera, p: Vec3, grain: Vec3) -> char {
        // `grain` may be a tiny numeric derivative: normalize before use.
        let g = grain.normalized();
        if g.length() < 0.5 {
            return '.';
        }
        let a = cam.project(self.to_world(p));
        let b = cam.project(self.to_world(p + g * 0.08));
        if let (Some(a), Some(b)) = (a, b) {
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            if dx.abs() + dy.abs() < 0.05 {
                return '.';
            }
            let deg = dy.atan2(dx).to_degrees();
            if deg.abs() < 24.0 {
                '-'
            } else if deg.abs() > 66.0 {
                '|'
            } else if deg > 0.0 {
                '\\'
            } else {
                '/'
            }
        } else {
            '.'
        }
    }

    /// Numeric surface normal of a parametric local-space sheet.
    fn normal(&self, f: impl Fn(f32, f32) -> Vec3, s: f32, t: f32) -> Vec3 {
        let e = 1e-3;
        let du = f(s + e, t) - f(s - e, t);
        let dv = f(s, t + e) - f(s, t - e);
        du.cross(dv).normalized()
    }

    /// Paint the whole fish into the glyph buffer.
    pub fn paint(&self, buf: &mut GlyphBuffer, cam: &Camera) {
        self.paint_tail(buf, cam);
        self.paint_dorsal(buf, cam);
        self.paint_anal(buf, cam);
        self.paint_pectorals(buf, cam);
        self.paint_filaments(buf, cam);
        self.paint_body(buf, cam);
        self.paint_eyes(buf, cam);
    }

    fn light_dir() -> Vec3 {
        // From above, slightly in front and to the left.
        Vec3::new(-0.25, 0.78, 0.58).normalized()
    }

    fn paint_body(&self, buf: &mut GlyphBuffer, cam: &Camera) {
        let nu = 130usize;
        let nth = 48usize;
        let light = Self::light_dir();
        for iu in 0..nu {
            let u = iu as f32 / (nu - 1) as f32;
            for it in 0..nth {
                let th = TAU * it as f32 / nth as f32;
                let p = self.body_point(u, th);
                let mut n = self.normal(|a, b| self.body_point(a, b), u, th);
                // Orient outward from the body axis.
                let axis = Vec3::new(Self::u_to_x(u), 0.0, self.wave(u));
                if n.dot(p - axis) < 0.0 {
                    n = -n;
                }
                let w = self.to_world(p);
                let nw = self.rotate(n).normalized();
                let Some(pr) = cam.project(w) else { continue };

                let view = cam.view_dir(w);
                let lam = nw.dot(light).max(0.0);
                let rim = 1.0 - nw.dot(view).abs();
                let bright = (0.46 + 0.88 * lam).clamp(0.0, 1.3);

                // Iridescent blue body with a moving sheen, cyan rim light,
                // and the classic crimson wash toward tail and belly.
                let sheen = 0.5 + 0.5 * (5.0 * u + 1.5 * th.sin() + 0.8 * self.phase).sin();
                let mut c = mix_color(DEEP_BLUE, ROYAL, sheen * 0.9);
                c = mix_color(c, CYAN, rim * rim * 0.65);
                let belly = smoothstep(0.1, -0.9, th.cos());
                let red = (smoothstep(0.50, 0.0, u) * 0.6 + belly * 0.30).min(0.7);
                c = mix_color(c, FIN_RED, red);
                c = scale_color(c, bright);

                // Ink density follows brightness with patches from the
                // sheen; sparse scale sparkle lifts a few cells one step.
                let mut idx =
                    ((bright * (0.75 + 0.45 * sheen)) * (BODY_RAMP.len() - 1) as f32) as usize;
                idx = idx.min(BODY_RAMP.len() - 1);
                if hash2(iu as i32, it as i32) > 0.93 {
                    idx = (idx + 1).min(BODY_RAMP.len() - 1);
                }
                let mut ch = BODY_RAMP[idx];

                // The grazing silhouette at the snout reads as an outline
                // parenthesis; keep the rest of the rim quiet.
                if rim > 0.93 && u > 0.78 {
                    let na = cam.project(w);
                    let nb = cam.project(w + nw * 0.05);
                    ch = match (na, nb) {
                        (Some(a), Some(b)) if b.x >= a.x => ')',
                        _ => '(',
                    };
                }
                buf.plot(pr.x, pr.y, pr.depth, ch, c);
            }
        }
    }

    fn paint_eyes(&self, buf: &mut GlyphBuffer, cam: &Camera) {
        for side in [-1.0f32, 1.0] {
            let local = Vec3::new(0.33, 0.05, side * (self.body_half_w(0.80) + 0.012));
            let w = self.to_world(local);
            let nw = self.rotate(Vec3::new(0.0, 0.25, side).normalized());
            // Only the camera-facing eye is visible.
            if nw.dot(cam.view_dir(w)) < 0.05 {
                continue;
            }
            if let Some(pr) = cam.project(w) {
                buf.plot(pr.x, pr.y, pr.depth - 0.01, 'O', EYE_DARK);
                let hl = self.to_world(local + Vec3::new(0.004, 0.012, side * 0.006));
                if let Some(h) = cam.project(hl) {
                    buf.plot(h.x, h.y, h.depth - 0.02, '.', [235, 242, 252]);
                }
            }
        }
    }

    /// Shared painter for the thin membrane fins (tail, dorsal, anal).
    /// `color` receives (s, t, brightness).
    fn paint_sheet(
        &self,
        buf: &mut GlyphBuffer,
        cam: &Camera,
        ns: usize,
        nt: usize,
        f: impl Fn(f32, f32) -> Vec3,
        color: impl Fn(f32, f32, f32) -> Color,
    ) {
        let light = Self::light_dir();
        let e = 1e-3;
        for is in 0..ns {
            let s = is as f32 / (ns - 1) as f32;
            for it in 0..nt {
                let t = it as f32 / (nt - 1) as f32;
                let p = f(s, t);
                let n = self.normal(&f, s, t);
                let nw = self.rotate(n).normalized();
                let lam = nw.dot(light).abs();
                let bright = 0.44 + 0.55 * lam;
                let grain = f(s, (t + e).min(1.0)) - f(s, t);
                let ch = self.stroke(cam, p, grain);
                let c = scale_color(color(s, t, bright), 1.0);
                self.plot(buf, cam, p, 0.005, ch, c);
            }
        }
    }

    fn paint_tail(&self, buf: &mut GlyphBuffer, cam: &Camera) {
        self.paint_sheet(
            buf,
            cam,
            78,
            26,
            |a, t| self.tail_point(a, t),
            |a, t, b| {
                let mut c = mix_color(DEEP_BLUE, FIN_RED, smoothstep(0.10, 0.80, t));
                // Crimson lobes, darker ruffled edge, pale blue streaks near base.
                c = mix_color(c, FIN_DARK, smoothstep(0.72, 1.0, t) * 0.55);
                let ray = 0.5 + 0.5 * (6.0 * a).sin();
                c = mix_color(c, CYAN, (1.0 - t) * 0.25 * ray);
                scale_color(c, b)
            },
        );
    }

    fn paint_dorsal(&self, buf: &mut GlyphBuffer, cam: &Camera) {
        self.paint_sheet(
            buf,
            cam,
            80,
            14,
            |s, t| self.dorsal_point(s, t),
            |s, t, b| {
                let mut c = mix_color(ROYAL, FIN_RED, smoothstep(0.25, 0.95, t));
                c = mix_color(c, CYAN, 0.20 * (1.0 - t) * (0.5 + 0.5 * (10.0 * s).sin()));
                scale_color(c, b)
            },
        );
    }

    fn paint_anal(&self, buf: &mut GlyphBuffer, cam: &Camera) {
        self.paint_sheet(
            buf,
            cam,
            80,
            18,
            |s, t| self.anal_point(s, t),
            |s, t, b| {
                let mut c = mix_color(DEEP_BLUE, FIN_RED, smoothstep(0.15, 0.85, t));
                c = mix_color(c, FIN_DARK, t * 0.30);
                let _ = s;
                scale_color(c, b)
            },
        );
    }

    fn paint_pectorals(&self, buf: &mut GlyphBuffer, cam: &Camera) {
        for side in [-1.0f32, 1.0] {
            let hinge = Vec3::new(0.30, -0.02, side * (self.body_half_w(0.30) + 0.005));
            let nw = self.rotate(Vec3::new(0.0, 0.0, side));
            // Skip the far-side fin; the body occludes it anyway and this
            // keeps the near flank readable.
            if nw.dot(cam.view_dir(self.to_world(hinge))) < 0.15 {
                continue;
            }
            self.paint_sheet(
                buf,
                cam,
                16,
                8,
                |a, t| self.pectoral_point(side, a * 2.0 - 1.0, t),
                |_, t, b| scale_color(mix_color(CYAN, FIN_RED, t * 0.6), b * 1.1),
            );
        }
    }

    fn paint_filaments(&self, buf: &mut GlyphBuffer, cam: &Camera) {
        let n = 30usize;
        for side in [-1.0f32, 1.0] {
            for i in 0..n {
                let t = i as f32 / (n - 1) as f32;
                let p = self.filament_point(side, t);
                let q = self.filament_point(side, (t + 0.03).min(1.0));
                let ch = self.stroke(cam, p, q - p);
                let c = scale_color(FIN_RED, 0.95 - 0.35 * t);
                self.plot(buf, cam, p, 0.0, ch, c);
            }
        }
    }
}

fn mix_color(a: Color, b: Color, t: f32) -> Color {
    let t = clamp01(t);
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
    ]
}

fn scale_color(c: Color, s: f32) -> Color {
    [
        (c[0] as f32 * s).clamp(0.0, 255.0) as u8,
        (c[1] as f32 * s).clamp(0.0, 255.0) as u8,
        (c[2] as f32 * s).clamp(0.0, 255.0) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;

    /// Projected x-extent of all non-blank cells.
    fn screen_x_span(buf: &GlyphBuffer) -> (usize, usize) {
        let mut lo = usize::MAX;
        let mut hi = 0;
        for y in 0..buf.height() {
            for x in 0..buf.width() {
                if buf.cell(x, y).ch != ' ' {
                    lo = lo.min(x);
                    hi = hi.max(x);
                }
            }
        }
        (lo, hi)
    }

    fn rendered_span(yaw: f32) -> usize {
        let mut fish = Betta::new();
        fish.yaw = yaw;
        let mut buf = GlyphBuffer::new(160, 48);
        let cam = Camera::new(160, 48);
        fish.paint(&mut buf, &cam);
        let (lo, hi) = screen_x_span(&buf);
        hi - lo
    }

    #[test]
    fn side_view_is_much_wider_than_front_view() {
        // Broadside (yaw = 0) the fish spans most of its length; facing the
        // camera (yaw = PI/2) perspective foreshortening must collapse it.
        let side = rendered_span(0.0);
        let front = rendered_span(std::f32::consts::FRAC_PI_2);
        assert!(
            side > front * 2,
            "expected strong foreshortening: side={side} front={front}"
        );
    }

    #[test]
    fn rendering_is_deterministic() {
        let mut a = Betta::new();
        let mut b = Betta::new();
        for _ in 0..600 {
            a.update(1.0 / 60.0);
            b.update(1.0 / 60.0);
        }
        let cam = Camera::new(120, 36);
        let (mut ba, mut bb) = (GlyphBuffer::new(120, 36), GlyphBuffer::new(120, 36));
        a.paint(&mut ba, &cam);
        b.paint(&mut bb, &cam);
        assert_eq!(ba.to_text(), bb.to_text());
    }

    #[test]
    fn fish_stays_in_tank_and_keeps_moving() {
        let mut fish = Betta::new();
        let start = fish.pos;
        for _ in 0..60_000 {
            fish.update(0.05);
            assert!(fish.pos.x.abs() <= 1.7 && fish.pos.y.abs() <= 0.65);
            assert!(fish.pos.z.abs() <= 0.85);
        }
        assert!((fish.pos - start).length() > 0.5, "fish should patrol");
    }

    #[test]
    fn body_is_dense_and_uses_density_ramp() {
        let fish = Betta::new(); // yaw = 0: broadside
        let mut buf = GlyphBuffer::new(160, 48);
        let cam = Camera::new(160, 48);
        fish.paint(&mut buf, &cam);
        assert!(buf.nonblank() > 400, "fish should cover many cells");
        let text = buf.to_text();
        assert!(text.contains('B') || text.contains('8') || text.contains('@'));
        // Fin strokes present.
        assert!(text.contains('/') || text.contains('\\') || text.contains('|'));
    }

    #[test]
    fn only_camera_facing_eye_is_drawn() {
        // Broadside facing +x with camera at +z: the fish's left eye (side
        // +1, local +z) faces the camera; the right one must not appear.
        let fish = Betta::new();
        let cam = Camera::new(120, 36);
        let mut buf = GlyphBuffer::new(120, 36);
        fish.paint_eyes(&mut buf, &cam);
        let eyes = buf.nonblank();
        assert!(
            (1..=4).contains(&eyes),
            "expected one small eye, got {eyes}"
        );
    }

    #[test]
    fn body_profile_is_deep_and_tapers() {
        let fish = Betta::new();
        let mid = fish.body_half_h(0.5);
        let peduncle = fish.body_half_h(0.0);
        let snout = fish.body_half_h(1.0);
        assert!(mid > peduncle * 2.0 && mid > snout * 2.0);
        assert!(fish.body_half_w(0.5) < fish.body_half_h(0.5) * 0.5);
    }

    #[test]
    fn tail_wave_moves_with_phase() {
        let mut fish = Betta::new();
        let a = fish.tail_point(0.0, 1.0);
        fish.phase = 2.0;
        let b = fish.tail_point(0.0, 1.0);
        assert!((a.z - b.z).abs() > 0.02, "tail tip should swing");
    }
}
