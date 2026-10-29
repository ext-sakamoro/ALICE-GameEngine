//! The previous path search of `navmesh` (f32, its own A*), kept only for the parity oracle
//! against the shared search in alice-navigation. Test-only; removed once parity is shown.

use super::{NavMesh, NavTriangle, NavVertex};
use crate::math::Vec3;

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
        other
            .f
            .partial_cmp(&self.f)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

pub fn navmesh_from_grid(grid: &[bool], width: usize, height: usize, cell_size: f32) -> NavMesh {
    assert_eq!(grid.len(), width * height, "grid size mismatch");
    let mut vertices = Vec::<NavVertex>::with_capacity((width + 1) * (height + 1));
    let v_at = |x: usize, y: usize| -> usize { y * (width + 1) + x };
    for y in 0..=height {
        for x in 0..=width {
            vertices.push(NavVertex {
                position: Vec3::new(x as f32 * cell_size, 0.0, y as f32 * cell_size),
            });
        }
    }
    let mut triangles = Vec::<NavTriangle>::with_capacity(width * height * 2);
    for y in 0..height {
        for x in 0..width {
            if grid[y * width + x] {
                continue;
            }
            let bl = v_at(x, y);
            let br = v_at(x + 1, y);
            let tl = v_at(x, y + 1);
            let tr = v_at(x + 1, y + 1);
            triangles.push(NavTriangle {
                indices: [bl as u32, br as u32, tl as u32],
                neighbors: [u32::MAX; 3],
            });
            triangles.push(NavTriangle {
                indices: [br as u32, tr as u32, tl as u32],
                neighbors: [u32::MAX; 3],
            });
        }
    }
    for i in 0..triangles.len() {
        for j in (i + 1)..triangles.len() {
            if share_edge(&triangles[i], &triangles[j]) {
                connect(&mut triangles, i, j);
            }
        }
    }
    NavMesh {
        vertices,
        triangles,
    }
}

fn share_edge(a: &NavTriangle, b: &NavTriangle) -> bool {
    a.indices.iter().filter(|v| b.indices.contains(v)).count() == 2
}

fn connect(triangles: &mut [NavTriangle], i: usize, j: usize) {
    for (from, to) in [(i, j), (j, i)] {
        for slot in &mut triangles[from].neighbors {
            if *slot == u32::MAX {
                *slot = to as u32;
                break;
            }
        }
    }
}

/// Parity of the shared search (alice-navigation) with the previous one, before the previous one
/// is removed. Costs are measured in f64 from the triangle centres, independently of both.
mod parity {
    use super::super::{a_star, navmesh_from_grid, NavMesh};
    use crate::math::Vec3;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
        }
    }

    fn centre(m: &NavMesh, t: usize) -> [f64; 3] {
        let c = m.triangle_center(t).unwrap();
        [f64::from(c.x()), f64::from(c.y()), f64::from(c.z())]
    }
    fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }
    fn neighbours(m: &NavMesh, t: usize) -> Vec<usize> {
        m.triangles[t]
            .neighbors
            .iter()
            .filter(|&&n| n != u32::MAX)
            .map(|&n| n as usize)
            .collect()
    }
    fn cost(m: &NavMesh, p: &[usize]) -> f64 {
        p.windows(2)
            .map(|w| dist(centre(m, w[0]), centre(m, w[1])))
            .sum()
    }
    /// Independent O(n²) Dijkstra in f64 from `s`.
    fn dijkstra(m: &NavMesh, s: usize) -> Vec<f64> {
        let n = m.triangles.len();
        let mut d = vec![f64::INFINITY; n];
        let mut done = vec![false; n];
        d[s] = 0.0;
        loop {
            let mut u = usize::MAX;
            for i in 0..n {
                if !done[i] && d[i].is_finite() && (u == usize::MAX || d[i] < d[u]) {
                    u = i;
                }
            }
            if u == usize::MAX {
                return d;
            }
            done[u] = true;
            for v in neighbours(m, u) {
                let c = d[u] + dist(centre(m, u), centre(m, v));
                if c < d[v] {
                    d[v] = c;
                }
            }
        }
    }
    /// The shortest path to `t` is unique *at f32 resolution* when, walking back from `t`, every
    /// node has exactly one predecessor within 1e-5 relative: the previous search summed in f32
    /// (about 6e-8 relative per step), so routes closer than that are not distinct to it.
    fn unique_shortest(m: &NavMesh, d: &[f64], s: usize, t: usize) -> bool {
        let mut v = t;
        while v != s {
            let tight: Vec<usize> = (0..m.triangles.len())
                .filter(|&u| neighbours(m, u).contains(&v))
                .filter(|&u| {
                    (d[u] + dist(centre(m, u), centre(m, v)) - d[v]).abs() <= 1e-5 * d[v].max(1.0)
                })
                .collect();
            if tight.len() != 1 {
                return false;
            }
            v = tight[0];
        }
        true
    }

    fn grid(seed: u64, w: usize, h: usize) -> Vec<bool> {
        let mut g = Lcg(seed);
        (0..w * h).map(|_| g.next() < 0.22).collect()
    }

    /// Heights on the vertices, so the search runs on a 3D surface.
    fn lifted(mut m: NavMesh, seed: u64) -> NavMesh {
        let mut g = Lcg(seed ^ 0x5eed);
        for v in &mut m.vertices {
            let p = v.position;
            v.position = Vec3::new(p.x(), (g.next() * 1.5) as f32, p.z());
        }
        m
    }

    #[test]
    fn grid_builder_matches_the_previous_one() {
        let mut compared = 0;
        for seed in 0..20 {
            let (w, h) = (6 + (seed as usize % 7), 5 + (seed as usize % 4));
            let g = grid(seed, w, h);
            for cell in [1.0f32, 0.5, 0.37] {
                let old = super::navmesh_from_grid(&g, w, h, cell);
                let new = navmesh_from_grid(&g, w, h, cell);
                assert_eq!(old.vertices.len(), new.vertices.len());
                for (a, b) in old.vertices.iter().zip(&new.vertices) {
                    assert_eq!(
                        [
                            a.position.x().to_bits(),
                            a.position.y().to_bits(),
                            a.position.z().to_bits()
                        ],
                        [
                            b.position.x().to_bits(),
                            b.position.y().to_bits(),
                            b.position.z().to_bits()
                        ]
                    );
                }
                assert_eq!(old.triangles.len(), new.triangles.len());
                for (i, (a, b)) in old.triangles.iter().zip(&new.triangles).enumerate() {
                    assert_eq!(a.indices, b.indices, "triangle {i}");
                    let (mut na, mut nb) = (a.neighbors, b.neighbors);
                    na.sort_unstable();
                    nb.sort_unstable();
                    assert_eq!(na, nb, "neighbour set of triangle {i}");
                    compared += 1;
                }
            }
        }
        assert!(compared > 0, "0 compared");
    }

    #[test]
    fn a_star_matches_the_previous_search() {
        let (mut compared, mut unique) = (0, 0);
        for seed in 0..20 {
            let (w, h) = (9 + (seed as usize % 5), 7 + (seed as usize % 3));
            let flat = navmesh_from_grid(&grid(100 + seed, w, h), w, h, 1.0);
            for m in [lifted(flat.clone(), seed), flat] {
                let n = m.triangles.len();
                if n < 4 {
                    continue;
                }
                for (s, t) in [(0, n - 1), (n / 4, 3 * n / 4), (n / 2, 1)] {
                    let old = super::a_star(&m, s, t);
                    let new = a_star(&m, s, t);
                    let d = dijkstra(&m, s);
                    assert_eq!(
                        old.is_some(),
                        new.is_some(),
                        "seed {seed}: reachability differs"
                    );
                    assert_eq!(new.is_some(), d[t].is_finite());
                    if let (Some(old), Some(new)) = (old, new) {
                        // the shared search is shortest in f64; the previous one was within f32 of it
                        assert!(
                            (cost(&m, &new) - d[t]).abs() <= 1e-9 * d[t].max(1.0),
                            "seed {seed}"
                        );
                        assert!(
                            (cost(&m, &old) - d[t]).abs() <= 1e-5 * d[t].max(1.0),
                            "seed {seed}"
                        );
                        if unique_shortest(&m, &d, s, t) {
                            assert_eq!(old, new, "seed {seed}: a unique shortest path differs");
                            unique += 1;
                        }
                        compared += 1;
                    }
                }
            }
        }
        eprintln!("[navmesh parity] compared {compared}, unique shortest {unique}");
        assert!(
            compared > 0 && unique > 0,
            "compared {compared}, unique {unique}"
        );
    }
}
