//! SDF + mesh hybrid navigation system.
//!
//! Unlike Fyrox's pure triangle-based navmesh, this module supports:
//! - Traditional polygon navmesh for pre-baked environments
//! - SDF-based distance queries for dynamic obstacle avoidance
//! - Hybrid mode: navmesh for coarse path, SDF for local steering

use crate::math::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// NavMeshVertex / NavTriangle / NavMesh
// ---------------------------------------------------------------------------

/// A vertex in the navigation mesh.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NavVertex {
    pub position: Vec3,
}

/// A triangle in the navigation mesh (indices into vertex array).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavTriangle {
    pub indices: [u32; 3],
    /// Adjacent triangles (`u32::MAX` = no neighbor).
    pub neighbors: [u32; 3],
}

/// A polygon navigation mesh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavMesh {
    pub vertices: Vec<NavVertex>,
    pub triangles: Vec<NavTriangle>,
}

impl NavMesh {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            vertices: Vec::new(),
            triangles: Vec::new(),
        }
    }

    /// Returns the triangle count.
    #[must_use]
    pub const fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// Returns the vertex count.
    #[must_use]
    pub const fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Finds the triangle containing the point (projected onto the mesh plane).
    /// Returns triangle index.
    #[must_use]
    pub fn find_triangle(&self, point: Vec3) -> Option<usize> {
        for (i, tri) in self.triangles.iter().enumerate() {
            let a = self.vertices[tri.indices[0] as usize].position;
            let b = self.vertices[tri.indices[1] as usize].position;
            let c = self.vertices[tri.indices[2] as usize].position;
            if point_in_triangle_xz(point, a, b, c) {
                return Some(i);
            }
        }
        None
    }

    /// Gets the center of a triangle.
    #[must_use]
    pub fn triangle_center(&self, tri_idx: usize) -> Option<Vec3> {
        let tri = self.triangles.get(tri_idx)?;
        let a = self.vertices[tri.indices[0] as usize].position;
        let b = self.vertices[tri.indices[1] as usize].position;
        let c = self.vertices[tri.indices[2] as usize].position;
        Some(Vec3::new(
            (a.x() + b.x() + c.x()) * (1.0 / 3.0),
            (a.y() + b.y() + c.y()) * (1.0 / 3.0),
            (a.z() + b.z() + c.z()) * (1.0 / 3.0),
        ))
    }
}

impl Default for NavMesh {
    fn default() -> Self {
        Self::new()
    }
}

/// XZ plane point-in-triangle test (ignores Y).
fn point_in_triangle_xz(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
    let d1 = sign_xz(p, a, b);
    let d2 = sign_xz(p, b, c);
    let d3 = sign_xz(p, c, a);
    let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
    let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);
    !(has_neg && has_pos)
}

fn sign_xz(p1: Vec3, p2: Vec3, p3: Vec3) -> f32 {
    (p1.x() - p3.x()).mul_add(p2.z() - p3.z(), -((p2.x() - p3.x()) * (p1.z() - p3.z())))
}

// ---------------------------------------------------------------------------
// SdfObstacle — dynamic SDF-based obstacle
// ---------------------------------------------------------------------------

/// A dynamic obstacle defined by an SDF volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdfObstacle {
    pub position: Vec3,
    pub radius: f32,
    pub sdf_json: String,
}

// ---------------------------------------------------------------------------
// NavPath
// ---------------------------------------------------------------------------

/// A navigation path (sequence of waypoints).
#[derive(Debug, Clone)]
pub struct NavPath {
    pub waypoints: Vec<Vec3>,
}

impl NavPath {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            waypoints: Vec::new(),
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.waypoints.is_empty()
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.waypoints.len()
    }

    /// Total path length.
    #[must_use]
    pub fn total_distance(&self) -> f32 {
        if self.waypoints.len() < 2 {
            return 0.0;
        }
        let mut total = 0.0;
        for pair in self.waypoints.windows(2) {
            total += pair[0].distance(pair[1]);
        }
        total
    }
}

impl Default for NavPath {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// NavAgent
// ---------------------------------------------------------------------------

/// Agent that follows a navigation path.
#[derive(Debug, Clone)]
pub struct NavAgent {
    pub position: Vec3,
    pub speed: f32,
    pub radius: f32,
    pub path: NavPath,
    pub waypoint_index: usize,
    pub reached_goal: bool,
}

impl NavAgent {
    #[must_use]
    pub const fn new(position: Vec3, speed: f32, radius: f32) -> Self {
        Self {
            position,
            speed,
            radius,
            path: NavPath::new(),
            waypoint_index: 0,
            reached_goal: false,
        }
    }

    /// Advances the agent along its path by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        if self.reached_goal || self.waypoint_index >= self.path.waypoints.len() {
            self.reached_goal = true;
            return;
        }

        let target = self.path.waypoints[self.waypoint_index];
        let to_target = target - self.position;
        let dist = to_target.length();
        let step = self.speed * dt;

        if dist <= self.radius.mul_add(0.5, step) {
            self.position = target;
            self.waypoint_index += 1;
            if self.waypoint_index >= self.path.waypoints.len() {
                self.reached_goal = true;
            }
        } else {
            let dir = to_target * (1.0 / dist);
            self.position = self.position + dir * step;
        }
    }

    /// Sets a new path and resets progress.
    pub fn set_path(&mut self, path: NavPath) {
        self.path = path;
        self.waypoint_index = 0;
        self.reached_goal = false;
    }
}

// ---------------------------------------------------------------------------
// A* pathfinding on navmesh triangles
// ---------------------------------------------------------------------------

/// A* pathfinding across navmesh triangles.
/// Returns a list of triangle indices from `start` to `goal`.
#[must_use]
pub fn a_star(mesh: &NavMesh, start: usize, goal: usize) -> Option<Vec<usize>> {
    if start >= mesh.triangles.len() || goal >= mesh.triangles.len() {
        return None;
    }
    if start == goal {
        return Some(vec![start]);
    }

    let tri_count = mesh.triangles.len();
    let mut open = std::collections::BinaryHeap::new();
    let mut g_score = vec![f32::MAX; tri_count];
    let mut came_from = vec![usize::MAX; tri_count];
    let mut closed = vec![false; tri_count];

    g_score[start] = 0.0;
    let goal_center = mesh.triangle_center(goal).unwrap_or(Vec3::ZERO);

    open.push(AStarNode { tri: start, f: 0.0 });

    while let Some(current) = open.pop() {
        if current.tri == goal {
            let mut path = vec![goal];
            let mut cur = goal;
            while came_from[cur] != usize::MAX {
                cur = came_from[cur];
                path.push(cur);
            }
            path.reverse();
            return Some(path);
        }

        if closed[current.tri] {
            continue;
        }
        closed[current.tri] = true;

        let tri = &mesh.triangles[current.tri];
        for &neighbor_idx in &tri.neighbors {
            if neighbor_idx == u32::MAX {
                continue;
            }
            let neighbor = neighbor_idx as usize;
            if neighbor >= tri_count || closed[neighbor] {
                continue;
            }

            let current_center = mesh.triangle_center(current.tri).unwrap_or(Vec3::ZERO);
            let neighbor_center = mesh.triangle_center(neighbor).unwrap_or(Vec3::ZERO);
            let tentative_g = g_score[current.tri] + current_center.distance(neighbor_center);

            if tentative_g < g_score[neighbor] {
                g_score[neighbor] = tentative_g;
                came_from[neighbor] = current.tri;
                let h = neighbor_center.distance(goal_center);
                open.push(AStarNode {
                    tri: neighbor,
                    f: tentative_g + h,
                });
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy)]
struct AStarNode {
    tri: usize,
    f: f32,
}

impl PartialEq for AStarNode {
    fn eq(&self, other: &Self) -> bool {
        self.tri == other.tri
    }
}
impl Eq for AStarNode {}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse ordering for min-heap behavior
        other
            .f
            .partial_cmp(&self.f)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

// ---------------------------------------------------------------------------
// SDF-based dynamic steering
// ---------------------------------------------------------------------------

/// Steers an agent away from a spherical SDF obstacle.
/// Returns the adjusted movement direction.
#[must_use]
pub fn sdf_steer(
    agent_pos: Vec3,
    desired_dir: Vec3,
    obstacle_pos: Vec3,
    obstacle_radius: f32,
    avoidance_strength: f32,
) -> Vec3 {
    let to_obstacle = obstacle_pos - agent_pos;
    let dist = to_obstacle.length();
    if dist >= obstacle_radius * 2.0 || dist < 1e-6 {
        return desired_dir;
    }
    let away = (agent_pos - obstacle_pos) * (1.0 / dist);
    let factor = (1.0 - dist / (obstacle_radius * 2.0)) * avoidance_strength;
    (desired_dir + away * factor).normalize()
}

// ---------------------------------------------------------------------------
// Crowd separation (RVO-style)
// ---------------------------------------------------------------------------

/// Applies separation forces between agents that are too close.
pub fn crowd_separation(agents: &mut [NavAgent], separation_radius: f32, strength: f32) {
    let positions: Vec<Vec3> = agents.iter().map(|a| a.position).collect();
    let count = agents.len();

    for i in 0..count {
        let mut push = Vec3::ZERO;
        for j in 0..count {
            if i == j {
                continue;
            }
            let diff = positions[i] - positions[j];
            let dist = diff.length();
            if dist < separation_radius && dist > 1e-6 {
                let force = (1.0 - dist / separation_radius) * strength;
                push = push + diff * (force / dist);
            }
        }
        agents[i].position = agents[i].position + push;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_mesh() -> NavMesh {
        NavMesh {
            vertices: vec![
                NavVertex {
                    position: Vec3::new(0.0, 0.0, 0.0),
                },
                NavVertex {
                    position: Vec3::new(10.0, 0.0, 0.0),
                },
                NavVertex {
                    position: Vec3::new(5.0, 0.0, 10.0),
                },
                NavVertex {
                    position: Vec3::new(10.0, 0.0, 10.0),
                },
            ],
            triangles: vec![
                NavTriangle {
                    indices: [0, 1, 2],
                    neighbors: [1, u32::MAX, u32::MAX],
                },
                NavTriangle {
                    indices: [1, 3, 2],
                    neighbors: [0, u32::MAX, u32::MAX],
                },
            ],
        }
    }

    #[test]
    fn navmesh_counts() {
        let nm = make_test_mesh();
        assert_eq!(nm.triangle_count(), 2);
        assert_eq!(nm.vertex_count(), 4);
    }

    #[test]
    fn find_triangle_inside() {
        let nm = make_test_mesh();
        let result = nm.find_triangle(Vec3::new(4.0, 0.0, 3.0));
        assert!(result.is_some());
    }

    #[test]
    fn find_triangle_outside() {
        let nm = make_test_mesh();
        let result = nm.find_triangle(Vec3::new(-10.0, 0.0, -10.0));
        assert!(result.is_none());
    }

    #[test]
    fn triangle_center() {
        let nm = make_test_mesh();
        let c = nm.triangle_center(0).unwrap();
        assert!((c.x() - 5.0).abs() < 1e-5);
    }

    #[test]
    fn navmesh_default() {
        let nm = NavMesh::default();
        assert_eq!(nm.triangle_count(), 0);
    }

    #[test]
    fn nav_path_empty() {
        let path = NavPath::new();
        assert!(path.is_empty());
        assert_eq!(path.total_distance(), 0.0);
    }

    #[test]
    fn nav_path_distance() {
        let path = NavPath {
            waypoints: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(3.0, 0.0, 0.0),
                Vec3::new(3.0, 4.0, 0.0),
            ],
        };
        assert!((path.total_distance() - 7.0).abs() < 1e-5);
    }

    #[test]
    fn nav_agent_reaches_goal() {
        let mut agent = NavAgent::new(Vec3::ZERO, 10.0, 0.5);
        agent.set_path(NavPath {
            waypoints: vec![Vec3::new(5.0, 0.0, 0.0)],
        });
        for _ in 0..100 {
            agent.update(0.1);
            if agent.reached_goal {
                break;
            }
        }
        assert!(agent.reached_goal);
    }

    #[test]
    fn nav_agent_multi_waypoint() {
        let mut agent = NavAgent::new(Vec3::ZERO, 100.0, 0.5);
        agent.set_path(NavPath {
            waypoints: vec![
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(3.0, 0.0, 0.0),
            ],
        });
        for _ in 0..100 {
            agent.update(0.1);
            if agent.reached_goal {
                break;
            }
        }
        assert!(agent.reached_goal);
        assert!((agent.position.x() - 3.0).abs() < 1.0);
    }

    #[test]
    fn nav_agent_no_path() {
        let mut agent = NavAgent::new(Vec3::ZERO, 10.0, 0.5);
        agent.update(1.0);
        assert!(agent.reached_goal);
    }

    #[test]
    fn sdf_obstacle_serialization() {
        let obs = SdfObstacle {
            position: Vec3::new(1.0, 0.0, 2.0),
            radius: 3.0,
            sdf_json: r#"{"type":"sphere"}"#.to_string(),
        };
        let json = serde_json::to_string(&obs).unwrap();
        let back: SdfObstacle = serde_json::from_str(&json).unwrap();
        assert_eq!(back.radius, 3.0);
    }

    #[test]
    fn point_in_triangle_xz_basic() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(10.0, 0.0, 0.0);
        let c = Vec3::new(5.0, 0.0, 10.0);
        assert!(point_in_triangle_xz(Vec3::new(5.0, 0.0, 3.0), a, b, c));
        assert!(!point_in_triangle_xz(Vec3::new(-5.0, 0.0, -5.0), a, b, c));
    }

    #[test]
    fn nav_agent_set_path_resets() {
        let mut agent = NavAgent::new(Vec3::ZERO, 10.0, 0.5);
        agent.reached_goal = true;
        agent.set_path(NavPath {
            waypoints: vec![Vec3::new(1.0, 0.0, 0.0)],
        });
        assert!(!agent.reached_goal);
        assert_eq!(agent.waypoint_index, 0);
    }

    #[test]
    fn a_star_direct_path() {
        let nm = make_test_mesh();
        let path = a_star(&nm, 0, 1);
        assert!(path.is_some());
        let p = path.unwrap();
        assert!(p.len() >= 2);
    }

    #[test]
    fn a_star_same_triangle() {
        let nm = make_test_mesh();
        let path = a_star(&nm, 0, 0);
        assert!(path.is_some());
        assert_eq!(path.unwrap().len(), 1);
    }

    #[test]
    fn a_star_no_path() {
        // Create two disconnected triangles
        let nm = NavMesh {
            vertices: vec![
                NavVertex {
                    position: Vec3::new(0.0, 0.0, 0.0),
                },
                NavVertex {
                    position: Vec3::new(1.0, 0.0, 0.0),
                },
                NavVertex {
                    position: Vec3::new(0.5, 0.0, 1.0),
                },
                NavVertex {
                    position: Vec3::new(10.0, 0.0, 10.0),
                },
                NavVertex {
                    position: Vec3::new(11.0, 0.0, 10.0),
                },
                NavVertex {
                    position: Vec3::new(10.5, 0.0, 11.0),
                },
            ],
            triangles: vec![
                NavTriangle {
                    indices: [0, 1, 2],
                    neighbors: [u32::MAX, u32::MAX, u32::MAX],
                },
                NavTriangle {
                    indices: [3, 4, 5],
                    neighbors: [u32::MAX, u32::MAX, u32::MAX],
                },
            ],
        };
        let path = a_star(&nm, 0, 1);
        assert!(path.is_none());
    }

    #[test]
    fn sdf_steering_away() {
        let obstacle_pos = Vec3::new(5.0, 0.0, 0.0);
        let agent_pos = Vec3::new(4.0, 0.0, 0.0);
        let desired_dir = Vec3::new(0.0, 0.0, 1.0);
        let steered = sdf_steer(agent_pos, desired_dir, obstacle_pos, 3.0, 2.0);
        // Should deflect away from obstacle (push in -X)
        assert!(steered.x() < 0.0);
    }

    #[test]
    fn sdf_steering_no_obstacle() {
        let steered = sdf_steer(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(100.0, 0.0, 0.0),
            3.0,
            1.0,
        );
        // No deflection needed — original direction preserved
        assert!((steered.x() - 1.0).abs() < 0.1);
    }

    #[test]
    fn crowd_agents_separate() {
        let mut agents = vec![
            NavAgent::new(Vec3::new(0.0, 0.0, 0.0), 5.0, 1.0),
            NavAgent::new(Vec3::new(0.5, 0.0, 0.0), 5.0, 1.0),
        ];
        crowd_separation(&mut agents, 2.0, 1.0);
        let dist = agents[0].position.distance(agents[1].position);
        assert!(dist > 0.5);
    }

    #[test]
    fn crowd_agents_far_apart_unchanged() {
        let mut agents = vec![
            NavAgent::new(Vec3::new(0.0, 0.0, 0.0), 5.0, 1.0),
            NavAgent::new(Vec3::new(100.0, 0.0, 0.0), 5.0, 1.0),
        ];
        let p0_before = agents[0].position;
        crowd_separation(&mut agents, 2.0, 1.0);
        assert!((agents[0].position.x() - p0_before.x()).abs() < 1e-6);
    }

    #[test]
    fn navmesh_find_path_integration() {
        let nm = make_test_mesh();
        let start = Vec3::new(3.0, 0.0, 2.0);
        let end = Vec3::new(8.0, 0.0, 5.0);
        let start_tri = nm.find_triangle(start);
        let end_tri = nm.find_triangle(end);
        assert!(start_tri.is_some());
        assert!(end_tri.is_some());
        let path = a_star(&nm, start_tri.unwrap(), end_tri.unwrap());
        assert!(path.is_some());
    }
}
