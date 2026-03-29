//! 2D scene support: sprites, tilemaps, and 2D physics.
//!
//! Provides a lightweight 2D layer (like Fyrox's dim2) that can coexist
//! with the 3D scene graph.

//!
//! ```rust
//! use alice_game_engine::scene2d::*;
//! let mut tm = TileMap::new(8, 8, 32.0);
//! tm.set(3, 4, TileDef { id: 1, solid: true });
//! assert!(tm.is_solid(3, 4));
//! ```
use crate::math::{Color, Vec2};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Sprite2D
// ---------------------------------------------------------------------------

/// A 2D sprite with position, rotation, scale, and rendering info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sprite2D {
    pub position: Vec2,
    pub rotation: f32,
    pub scale: Vec2,
    pub anchor: Vec2,
    pub color: Color,
    pub texture_id: u32,
    pub uv_rect: [f32; 4],
    pub z_order: i32,
    pub visible: bool,
    pub flip_x: bool,
    pub flip_y: bool,
}

impl Sprite2D {
    #[must_use]
    pub const fn new(texture_id: u32) -> Self {
        Self {
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
            anchor: Vec2::new(0.5, 0.5),
            color: Color::WHITE,
            texture_id,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            z_order: 0,
            visible: true,
            flip_x: false,
            flip_y: false,
        }
    }
}

// ---------------------------------------------------------------------------
// TileMap
// ---------------------------------------------------------------------------

/// A single tile definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileDef {
    pub id: u16,
    pub solid: bool,
}

impl TileDef {
    pub const EMPTY: Self = Self {
        id: 0,
        solid: false,
    };
}

/// A 2D tile map with fixed-size cells.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileMap {
    pub width: u32,
    pub height: u32,
    pub tile_size: f32,
    tiles: Vec<TileDef>,
}

impl TileMap {
    #[must_use]
    pub fn new(width: u32, height: u32, tile_size: f32) -> Self {
        let count = (width * height) as usize;
        Self {
            width,
            height,
            tile_size,
            tiles: vec![TileDef::EMPTY; count],
        }
    }

    /// Sets a tile at (x, y).
    pub fn set(&mut self, x: u32, y: u32, tile: TileDef) {
        if x < self.width && y < self.height {
            self.tiles[(y * self.width + x) as usize] = tile;
        }
    }

    /// Gets a tile at (x, y).
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> TileDef {
        if x < self.width && y < self.height {
            self.tiles[(y * self.width + x) as usize]
        } else {
            TileDef::EMPTY
        }
    }

    /// Converts world position to tile coordinates.
    #[must_use]
    pub fn world_to_tile(&self, pos: Vec2) -> (i32, i32) {
        let tx = (pos.x() / self.tile_size).floor() as i32;
        let ty = (pos.y() / self.tile_size).floor() as i32;
        (tx, ty)
    }

    /// Converts tile coordinates to world center.
    #[must_use]
    pub fn tile_to_world(&self, tx: u32, ty: u32) -> Vec2 {
        let half = self.tile_size * 0.5;
        Vec2::new(
            (tx as f32).mul_add(self.tile_size, half),
            (ty as f32).mul_add(self.tile_size, half),
        )
    }

    /// Returns true if the tile at (x, y) is solid.
    #[must_use]
    pub fn is_solid(&self, x: u32, y: u32) -> bool {
        self.get(x, y).solid
    }

    /// Returns all solid tile positions as AABB centers.
    #[must_use]
    pub fn solid_positions(&self) -> Vec<Vec2> {
        let mut result = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.is_solid(x, y) {
                    result.push(self.tile_to_world(x, y));
                }
            }
        }
        result
    }

    /// Total tile count.
    #[must_use]
    pub const fn tile_count(&self) -> usize {
        self.tiles.len()
    }
}

// ---------------------------------------------------------------------------
// AABB2 — 2D bounding box
// ---------------------------------------------------------------------------

/// 2D axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Aabb2 {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb2 {
    #[inline]
    #[must_use]
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    #[inline]
    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        self.min.x() <= other.max.x()
            && self.max.x() >= other.min.x()
            && self.min.y() <= other.max.y()
            && self.max.y() >= other.min.y()
    }

    #[inline]
    #[must_use]
    pub fn contains_point(&self, p: Vec2) -> bool {
        p.x() >= self.min.x()
            && p.x() <= self.max.x()
            && p.y() >= self.min.y()
            && p.y() <= self.max.y()
    }

    #[inline]
    #[must_use]
    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    #[inline]
    #[must_use]
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }
}

// ---------------------------------------------------------------------------
// Physics2D — simple 2D AABB collision
// ---------------------------------------------------------------------------

/// A 2D physics body.
#[derive(Debug, Clone)]
pub struct Body2D {
    pub position: Vec2,
    pub velocity: Vec2,
    pub half_size: Vec2,
    pub mass: f32,
    pub is_static: bool,
}

impl Body2D {
    #[must_use]
    pub const fn new(position: Vec2, half_size: Vec2, mass: f32) -> Self {
        Self {
            position,
            velocity: Vec2::ZERO,
            half_size,
            mass,
            is_static: false,
        }
    }

    #[must_use]
    pub const fn new_static(position: Vec2, half_size: Vec2) -> Self {
        Self {
            position,
            velocity: Vec2::ZERO,
            half_size,
            mass: 0.0,
            is_static: true,
        }
    }

    #[must_use]
    pub fn aabb(&self) -> Aabb2 {
        Aabb2::new(
            self.position - self.half_size,
            self.position + self.half_size,
        )
    }
}

/// 2D contact info.
#[derive(Debug, Clone, Copy)]
pub struct Contact2D {
    pub body_a: usize,
    pub body_b: usize,
    pub normal: Vec2,
    pub penetration: f32,
}

/// Detects AABB overlaps between 2D bodies.
#[must_use]
pub fn detect_2d_collisions(bodies: &[Body2D]) -> Vec<Contact2D> {
    let mut contacts = Vec::new();
    let len = bodies.len();
    for i in 0..len {
        let a = bodies[i].aabb();
        for j in (i + 1)..len {
            let b = bodies[j].aabb();
            if a.intersects(&b) {
                // Compute overlap
                let ox = (a.size().x() + b.size().x()).mul_add(
                    0.5,
                    -(bodies[i].position.x() - bodies[j].position.x()).abs(),
                );
                let oy = (a.size().y() + b.size().y()).mul_add(
                    0.5,
                    -(bodies[i].position.y() - bodies[j].position.y()).abs(),
                );

                if ox > 0.0 && oy > 0.0 {
                    let (normal, pen) = if ox < oy {
                        let sign = if bodies[i].position.x() < bodies[j].position.x() {
                            -1.0
                        } else {
                            1.0
                        };
                        (Vec2::new(sign, 0.0), ox)
                    } else {
                        let sign = if bodies[i].position.y() < bodies[j].position.y() {
                            -1.0
                        } else {
                            1.0
                        };
                        (Vec2::new(0.0, sign), oy)
                    };
                    contacts.push(Contact2D {
                        body_a: i,
                        body_b: j,
                        normal,
                        penetration: pen,
                    });
                }
            }
        }
    }
    contacts
}

/// SDF 2D circle collision test.
#[must_use]
pub fn sdf2d_circle_test(sdf_eval: &dyn Fn(Vec2) -> f32, center: Vec2, radius: f32) -> bool {
    sdf_eval(center) < radius
}

// ---------------------------------------------------------------------------
// Scene2D — collection of sprites
// ---------------------------------------------------------------------------

/// A simple 2D scene that holds sprites and provides z-order sorting.
pub struct Scene2D {
    pub sprites: Vec<Sprite2D>,
}

impl Scene2D {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sprites: Vec::new(),
        }
    }

    pub fn add(&mut self, sprite: Sprite2D) -> usize {
        self.sprites.push(sprite);
        self.sprites.len() - 1
    }

    /// Returns sprite indices sorted by `z_order` (ascending).
    #[must_use]
    pub fn render_order(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.sprites.len())
            .filter(|&i| self.sprites[i].visible)
            .collect();
        indices.sort_by_key(|&i| self.sprites[i].z_order);
        indices
    }

    #[must_use]
    pub const fn count(&self) -> usize {
        self.sprites.len()
    }
}

impl Default for Scene2D {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite2d_new() {
        let s = Sprite2D::new(42);
        assert_eq!(s.texture_id, 42);
        assert!(s.visible);
        assert_eq!(s.z_order, 0);
    }

    #[test]
    fn tilemap_set_get() {
        let mut tm = TileMap::new(10, 10, 32.0);
        tm.set(3, 4, TileDef { id: 5, solid: true });
        assert_eq!(tm.get(3, 4).id, 5);
        assert!(tm.get(3, 4).solid);
        assert_eq!(tm.get(0, 0).id, 0);
    }

    #[test]
    fn tilemap_out_of_bounds() {
        let tm = TileMap::new(5, 5, 16.0);
        assert_eq!(tm.get(10, 10), TileDef::EMPTY);
    }

    #[test]
    fn tilemap_world_to_tile() {
        let tm = TileMap::new(10, 10, 32.0);
        let (tx, ty) = tm.world_to_tile(Vec2::new(50.0, 70.0));
        assert_eq!(tx, 1);
        assert_eq!(ty, 2);
    }

    #[test]
    fn tilemap_tile_to_world() {
        let tm = TileMap::new(10, 10, 32.0);
        let p = tm.tile_to_world(0, 0);
        assert!((p.x() - 16.0).abs() < 1e-6);
        assert!((p.y() - 16.0).abs() < 1e-6);
    }

    #[test]
    fn tilemap_solid_positions() {
        let mut tm = TileMap::new(3, 3, 16.0);
        tm.set(0, 0, TileDef { id: 1, solid: true });
        tm.set(2, 2, TileDef { id: 2, solid: true });
        let solids = tm.solid_positions();
        assert_eq!(solids.len(), 2);
    }

    #[test]
    fn tilemap_tile_count() {
        let tm = TileMap::new(4, 5, 8.0);
        assert_eq!(tm.tile_count(), 20);
    }

    #[test]
    fn aabb2_intersects() {
        let a = Aabb2::new(Vec2::ZERO, Vec2::ONE);
        let b = Aabb2::new(Vec2::new(0.5, 0.5), Vec2::new(2.0, 2.0));
        assert!(a.intersects(&b));
    }

    #[test]
    fn aabb2_no_intersect() {
        let a = Aabb2::new(Vec2::ZERO, Vec2::ONE);
        let b = Aabb2::new(Vec2::new(5.0, 5.0), Vec2::new(6.0, 6.0));
        assert!(!a.intersects(&b));
    }

    #[test]
    fn aabb2_contains_point() {
        let a = Aabb2::new(Vec2::ZERO, Vec2::new(10.0, 10.0));
        assert!(a.contains_point(Vec2::new(5.0, 5.0)));
        assert!(!a.contains_point(Vec2::new(-1.0, 5.0)));
    }

    #[test]
    fn aabb2_center_size() {
        let a = Aabb2::new(Vec2::new(2.0, 4.0), Vec2::new(6.0, 8.0));
        assert!((a.center().x() - 4.0).abs() < 1e-6);
        assert!((a.size().y() - 4.0).abs() < 1e-6);
    }

    #[test]
    fn body2d_aabb() {
        let b = Body2D::new(Vec2::new(5.0, 5.0), Vec2::new(1.0, 1.0), 1.0);
        let aabb = b.aabb();
        assert!((aabb.min.x() - 4.0).abs() < 1e-6);
        assert!((aabb.max.x() - 6.0).abs() < 1e-6);
    }

    #[test]
    fn detect_2d_overlap() {
        let bodies = vec![
            Body2D::new(Vec2::ZERO, Vec2::ONE, 1.0),
            Body2D::new(Vec2::new(1.0, 0.0), Vec2::ONE, 1.0),
        ];
        let contacts = detect_2d_collisions(&bodies);
        assert_eq!(contacts.len(), 1);
        assert!(contacts[0].penetration > 0.0);
    }

    #[test]
    fn detect_2d_no_overlap() {
        let bodies = vec![
            Body2D::new(Vec2::ZERO, Vec2::ONE, 1.0),
            Body2D::new(Vec2::new(10.0, 0.0), Vec2::ONE, 1.0),
        ];
        let contacts = detect_2d_collisions(&bodies);
        assert!(contacts.is_empty());
    }

    #[test]
    fn sdf2d_circle_hit() {
        let sdf = |p: Vec2| p.length() - 2.0;
        assert!(sdf2d_circle_test(&sdf, Vec2::new(1.0, 0.0), 1.5));
    }

    #[test]
    fn sdf2d_circle_miss() {
        let sdf = |p: Vec2| p.length() - 2.0;
        assert!(!sdf2d_circle_test(&sdf, Vec2::new(5.0, 0.0), 0.5));
    }

    #[test]
    fn scene2d_render_order() {
        let mut scene = Scene2D::new();
        let mut s1 = Sprite2D::new(0);
        s1.z_order = 10;
        let mut s2 = Sprite2D::new(1);
        s2.z_order = 5;
        let mut s3 = Sprite2D::new(2);
        s3.z_order = 20;
        scene.add(s1);
        scene.add(s2);
        scene.add(s3);
        let order = scene.render_order();
        assert_eq!(order, vec![1, 0, 2]);
    }

    #[test]
    fn scene2d_hidden_sprites() {
        let mut scene = Scene2D::new();
        let mut s = Sprite2D::new(0);
        s.visible = false;
        scene.add(s);
        scene.add(Sprite2D::new(1));
        let order = scene.render_order();
        assert_eq!(order.len(), 1);
    }

    #[test]
    fn scene2d_count() {
        let mut scene = Scene2D::new();
        scene.add(Sprite2D::new(0));
        scene.add(Sprite2D::new(1));
        assert_eq!(scene.count(), 2);
    }

    #[test]
    fn sprite2d_flip() {
        let mut s = Sprite2D::new(0);
        s.flip_x = true;
        s.flip_y = true;
        assert!(s.flip_x);
        assert!(s.flip_y);
    }

    #[test]
    fn tile_def_empty() {
        assert_eq!(TileDef::EMPTY.id, 0);
        assert!(!TileDef::EMPTY.solid);
    }
}
