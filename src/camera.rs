//! Perspective camera mapping the 3D tank onto terminal character cells.
//!
//! World space: the tank is centered at the origin, +x right, +y up, +z
//! toward the camera. The camera sits on the +z axis looking at the origin.
//! Because terminal cells are roughly twice as tall as they are wide, the
//! vertical projection is scaled by `CELL_ASPECT` so circles stay round.

use crate::math::Vec3;

/// Height/width ratio of a terminal character cell. A world unit therefore
/// covers half as many rows as columns.
pub const CELL_ASPECT: f32 = 2.0;

/// Frame slightly tighter than the tank so the scene reads like a camera
/// looking *into* the water rather than a diagram of the whole tank.
const ZOOM: f32 = 1.15;

/// World-space half extents of the tank; geometry is kept inside these.
pub const TANK_X: f32 = 2.2;
pub const TANK_Y: f32 = 1.05;
pub const TANK_Z: f32 = 1.05;

pub struct Projection {
    /// Screen position in character cells.
    pub x: f32,
    pub y: f32,
    /// Distance from the camera; smaller is nearer (used for the z-buffer).
    pub depth: f32,
}

pub struct Camera {
    /// Distance from the camera to the tank center.
    dist: f32,
    /// Focal length in character cells (horizontal).
    focal: f32,
    cx: f32,
    cy: f32,
}

impl Camera {
    /// Build a camera that fits the tank into a `width` x `height` cell grid
    /// with a small margin.
    pub fn new(width: usize, height: usize) -> Self {
        let dist = 5.4;
        // Fit horizontally and vertically; take the tighter constraint. A
        // world unit spans `focal/depth` columns and `focal/depth/
        // CELL_ASPECT` rows, so the vertical fit multiplies by CELL_ASPECT.
        let fx = width as f32 * 0.46 * dist / TANK_X;
        let fy = height as f32 * 0.46 * dist * CELL_ASPECT / (TANK_Y + 0.15);
        Self {
            dist,
            focal: fx.min(fy) * ZOOM,
            cx: width as f32 * 0.5,
            cy: height as f32 * 0.52,
        }
    }

    /// Project a world point to screen cells. Returns `None` when the point
    /// is behind (or too close to) the camera.
    pub fn project(&self, p: Vec3) -> Option<Projection> {
        let depth = self.dist - p.z;
        if depth < 0.8 {
            return None;
        }
        Some(Projection {
            x: self.cx + self.focal * p.x / depth,
            y: self.cy - self.focal * p.y / (depth * CELL_ASPECT),
            depth,
        })
    }

    /// Unit vector from a world point toward the camera.
    pub fn view_dir(&self, p: Vec3) -> Vec3 {
        (Vec3::new(0.0, 0.0, self.dist) - p).normalized()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn central_region_fits_on_screen() {
        // The camera zooms tighter than the tank (edges may crop), but the
        // volume the fish actually patrols must always be visible.
        let (w, h) = (120usize, 36usize);
        let cam = Camera::new(w, h);
        for ix in [-1.0f32, 1.0] {
            for iy in [-1.0f32, 1.0] {
                for iz in [-1.0f32, 1.0] {
                    let p = Vec3::new(ix * 1.7, iy * 0.6, iz * 0.85);
                    let pr = cam.project(p).expect("point in front of camera");
                    assert!(
                        pr.x >= 0.0 && pr.x < w as f32 && pr.y >= 0.0 && pr.y < h as f32,
                        "point {p:?} projected off-screen to ({},{})",
                        pr.x,
                        pr.y
                    );
                }
            }
        }
    }

    #[test]
    fn nearer_objects_appear_larger() {
        let cam = Camera::new(120, 36);
        // Same world segment at two depths: nearer one spans more cells.
        let span = |z: f32| {
            let a = cam.project(Vec3::new(-0.5, 0.0, z)).unwrap();
            let b = cam.project(Vec3::new(0.5, 0.0, z)).unwrap();
            b.x - a.x
        };
        assert!(span(0.8) > span(-0.8) * 1.2);
    }

    #[test]
    fn vertical_scale_accounts_for_cell_aspect() {
        let cam = Camera::new(120, 36);
        let a = cam.project(Vec3::new(0.0, -0.5, 0.0)).unwrap();
        let b = cam.project(Vec3::new(0.0, 0.5, 0.0)).unwrap();
        let va = cam.project(Vec3::new(-0.5, 0.0, 0.0)).unwrap();
        let vb = cam.project(Vec3::new(0.5, 0.0, 0.0)).unwrap();
        // One world unit vertically covers 1/CELL_ASPECT the cells of one
        // world unit horizontally (cells are taller than wide).
        let ratio = (a.y - b.y) / (vb.x - va.x);
        assert!((ratio - 1.0 / CELL_ASPECT).abs() < 1e-4);
    }
}
