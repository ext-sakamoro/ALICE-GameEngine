//! Shader management: WGSL source storage, compilation cache.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// ShaderSource
// ---------------------------------------------------------------------------

/// A named WGSL shader source.
#[derive(Debug, Clone)]
pub struct ShaderSource {
    pub name: String,
    pub wgsl: String,
    pub stage: ShaderStage,
}

/// Shader stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}

impl ShaderSource {
    #[must_use]
    pub fn new(name: &str, wgsl: &str, stage: ShaderStage) -> Self {
        Self {
            name: name.to_string(),
            wgsl: wgsl.to_string(),
            stage,
        }
    }

    /// Validates basic WGSL structure (entry point presence).
    #[must_use]
    pub fn has_entry_point(&self) -> bool {
        match self.stage {
            ShaderStage::Vertex => self.wgsl.contains("@vertex"),
            ShaderStage::Fragment => self.wgsl.contains("@fragment"),
            ShaderStage::Compute => self.wgsl.contains("@compute"),
        }
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.wgsl.lines().count()
    }
}

// ---------------------------------------------------------------------------
// ShaderCache
// ---------------------------------------------------------------------------

/// Caches compiled shader sources by name.
pub struct ShaderCache {
    shaders: HashMap<String, ShaderSource>,
}

impl ShaderCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            shaders: HashMap::new(),
        }
    }

    /// Adds a shader to the cache.
    pub fn add(&mut self, shader: ShaderSource) {
        self.shaders.insert(shader.name.clone(), shader);
    }

    /// Gets a shader by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ShaderSource> {
        self.shaders.get(name)
    }

    /// Removes a shader.
    pub fn remove(&mut self, name: &str) -> Option<ShaderSource> {
        self.shaders.remove(name)
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.shaders.len()
    }

    /// Returns all shader names.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.shaders
            .keys()
            .map(std::string::String::as_str)
            .collect()
    }
}

impl Default for ShaderCache {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Built-in shaders
// ---------------------------------------------------------------------------

/// Built-in `GBuffer` vertex shader (WGSL).
pub const GBUFFER_VERTEX_WGSL: &str = r"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct Uniforms {
    model: mat4x4<f32>,
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = uniforms.model * vec4<f32>(in.position, 1.0);
    out.world_position = world_pos.xyz;
    out.world_normal = (uniforms.model * vec4<f32>(in.normal, 0.0)).xyz;
    out.clip_position = uniforms.projection * uniforms.view * world_pos;
    out.uv = in.uv;
    return out;
}
";

/// Built-in `GBuffer` fragment shader (WGSL).
pub const GBUFFER_FRAGMENT_WGSL: &str = r"
struct FragmentInput {
    @location(0) world_normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct GBufferOutput {
    @location(0) albedo: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) emission: vec4<f32>,
    @location(3) material: vec4<f32>,
};

struct Material {
    albedo: vec4<f32>,
    metallic: f32,
    roughness: f32,
    emission_strength: f32,
    _pad: f32,
};

@group(1) @binding(0) var<uniform> material: Material;

@fragment
fn fs_main(in: FragmentInput) -> GBufferOutput {
    var out: GBufferOutput;
    out.albedo = material.albedo;
    out.normal = vec4<f32>(normalize(in.world_normal) * 0.5 + 0.5, 1.0);
    out.emission = vec4<f32>(material.albedo.rgb * material.emission_strength, 1.0);
    out.material = vec4<f32>(material.metallic, material.roughness, 1.0, 0.0);
    return out;
}
";

/// Built-in SDF raymarch fragment shader (WGSL).
pub const SDF_RAYMARCH_FRAGMENT_WGSL: &str = r"
struct RaymarchUniforms {
    camera_pos: vec3<f32>,
    _pad0: f32,
    camera_dir: vec3<f32>,
    _pad1: f32,
    resolution: vec2<f32>,
    time: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> u: RaymarchUniforms;

fn sdf_sphere(p: vec3<f32>, r: f32) -> f32 {
    return length(p) - r;
}

fn sdf_box(p: vec3<f32>, b: vec3<f32>) -> f32 {
    let q = abs(p) - b;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (frag_coord.xy / u.resolution) * 2.0 - 1.0;
    let rd = normalize(vec3<f32>(uv.x, uv.y, -1.0));
    var t: f32 = 0.0;
    for (var i: i32 = 0; i < 64; i++) {
        let p = u.camera_pos + rd * t;
        let d = sdf_sphere(p, 1.0);
        if d < 0.001 {
            let n = normalize(p);
            let light = max(dot(n, normalize(vec3<f32>(1.0, 1.0, 1.0))), 0.1);
            return vec4<f32>(vec3<f32>(light), 1.0);
        }
        t += d;
        if t > 100.0 { break; }
    }
    return vec4<f32>(0.05, 0.05, 0.1, 1.0);
}
";

/// Fullscreen triangle vertex shader.
pub const FULLSCREEN_VERTEX_WGSL: &str = r"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}
";

/// GPU BVH interior-node refit compute shader. Dispatched
/// bottom-up: callers pass in a slice of node indices belonging to
/// one tree level via `level_indices`, and each invocation unions
/// the bounds of its two children.
pub const GPU_BVH_INTERIOR_REFIT_COMPUTE_WGSL: &str = r"
struct BvhNode {
    bounds_min: vec3<f32>,
    left: u32,
    bounds_max: vec3<f32>,
    right: u32,
    primitive_start: u32,
    primitive_count: u32,
    _pad: vec2<u32>,
};

struct InteriorParams {
    level_count: u32,
    _pad: vec3<u32>,
};

@group(0) @binding(0) var<uniform> params: InteriorParams;
@group(0) @binding(1) var<storage, read> level_indices: array<u32>;
@group(0) @binding(2) var<storage, read_write> nodes: array<BvhNode>;

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= params.level_count {
        return;
    }
    let node_idx = level_indices[i];
    let node = nodes[node_idx];
    // Leaves keep their bounds (already refit by gpu_bvh_refit_compute).
    if node.primitive_count > 0u {
        return;
    }
    let left = nodes[node.left];
    let right = nodes[node.right];
    nodes[node_idx].bounds_min = min(left.bounds_min, right.bounds_min);
    nodes[node_idx].bounds_max = max(left.bounds_max, right.bounds_max);
}
";

/// GPU BVH refit compute shader scaffold. Reads a flat `BvhNode`
/// storage buffer + per-primitive `Aabb` buffer, and re-computes each
/// internal node's bounding box bottom-up. Pair with
/// [`crate::gpu_bvh::Bvh`] (`primitive_order` already Morton-sorted on
/// the CPU).
///
/// This is the **scaffold** path: it only refits an existing
/// hierarchy. A future PR adds the parallel Morton-build step (=
/// "Karras 2012") so the entire BVH can be rebuilt on the GPU each
/// frame for dynamic geometry.
pub const GPU_BVH_REFIT_COMPUTE_WGSL: &str = r"
struct Aabb {
    min: vec3<f32>,
    _pad0: f32,
    max: vec3<f32>,
    _pad1: f32,
};

struct BvhNode {
    bounds_min: vec3<f32>,
    left: u32,
    bounds_max: vec3<f32>,
    right: u32,
    primitive_start: u32,
    primitive_count: u32,
    _pad: vec2<u32>,
};

struct BvhParams {
    node_count: u32,
    _pad: vec3<u32>,
};

@group(0) @binding(0) var<uniform> params: BvhParams;
@group(0) @binding(1) var<storage, read> primitive_aabbs: array<Aabb>;
@group(0) @binding(2) var<storage, read_write> nodes: array<BvhNode>;

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if idx >= params.node_count {
        return;
    }
    let node = nodes[idx];
    // Leaf: union primitives in [primitive_start, primitive_start + count).
    if node.primitive_count > 0u {
        var lo = vec3<f32>(1e30, 1e30, 1e30);
        var hi = vec3<f32>(-1e30, -1e30, -1e30);
        for (var i = 0u; i < node.primitive_count; i++) {
            let p = primitive_aabbs[node.primitive_start + i];
            lo = min(lo, p.min);
            hi = max(hi, p.max);
        }
        nodes[idx].bounds_min = lo;
        nodes[idx].bounds_max = hi;
    }
    // Interior bounds depend on children — caller should dispatch in
    // post-order. This scaffold updates only leaves; the bottom-up
    // refit driver is left for the next PR.
}
";

/// Cubemap-face sky shader. Draws a fullscreen triangle into one face
/// of an `Rgba16Float` cubemap and colors each pixel by the world-
/// space direction reconstructed from the screen UV + the supplied
/// face-camera inverse view-projection matrix.
pub const CUBEMAP_SKY_FRAGMENT_WGSL: &str = r"
struct SkyUniforms {
    inv_view_projection: mat4x4<f32>,
    sun_direction: vec3<f32>,
    _pad: f32,
    horizon_color: vec3<f32>,
    _pad2: f32,
    zenith_color: vec3<f32>,
    _pad3: f32,
};

@group(0) @binding(0) var<uniform> sky: SkyUniforms;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    // Re-project the screen UV through the per-face inverse VP to get
    // the world-space ray direction we are looking at.
    let ndc = vec3<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 1.0);
    let world_h = sky.inv_view_projection * vec4<f32>(ndc, 1.0);
    let dir = normalize(world_h.xyz / max(world_h.w, 1e-6));

    // Smooth horizon→zenith gradient, with a soft sun disc.
    let t = clamp(dir.y * 0.5 + 0.5, 0.0, 1.0);
    let base = mix(sky.horizon_color, sky.zenith_color, t);
    let sun = max(dot(dir, normalize(sky.sun_direction)), 0.0);
    let sun_disc = pow(sun, 256.0) * 12.0;
    let color = base + vec3<f32>(sun_disc);
    return vec4<f32>(color, 1.0);
}
";

/// Tiled light culling compute shader.
///
/// One workgroup per `(group_count / WORKGROUP_SIZE)` rows of lights;
/// each invocation tests one (light, tile) pair and atomically appends
/// to the per-tile index list when the sphere overlaps the tile AABB.
/// Mirrors `light_culling::TiledLightCuller::cull` so the GPU output
/// matches the CPU reference bit-for-bit.
pub const LIGHT_CULLING_COMPUTE_WGSL: &str = r"
struct LightSphere {
    position: vec3<f32>,
    radius: f32,
};

struct CullParams {
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    screen_w: u32,
    screen_h: u32,
    tile_size: u32,
    tile_count_x: u32,
    tile_count_y: u32,
    light_count: u32,
    max_lights_per_tile: u32,
    _pad: u32,
};

@group(0) @binding(0) var<uniform> params: CullParams;
@group(0) @binding(1) var<storage, read> lights: array<LightSphere>;
@group(0) @binding(2) var<storage, read_write> tile_counts: array<atomic<u32>>;
@group(0) @binding(3) var<storage, read_write> tile_indices: array<u32>;

fn ndc_to_tile(ndc_x: f32, ndc_y: f32) -> vec2<i32> {
    let px = (ndc_x * 0.5 + 0.5) * f32(params.screen_w);
    let py = (0.5 - ndc_y * 0.5) * f32(params.screen_h);
    return vec2<i32>(i32(px) / i32(params.tile_size), i32(py) / i32(params.tile_size));
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let light_idx = gid.x;
    if light_idx >= params.light_count {
        return;
    }
    let light = lights[light_idx];
    if light.radius <= 0.0 {
        return;
    }

    // Project the bounding sphere's 8 AABB corners to screen, take min/max.
    var min_t = vec2<i32>(i32(params.tile_count_x), i32(params.tile_count_y));
    var max_t = vec2<i32>(-1, -1);
    let signs = array<f32, 2>(-1.0, 1.0);
    var any_in_front = false;
    for (var sx = 0; sx < 2; sx++) {
        for (var sy = 0; sy < 2; sy++) {
            for (var sz = 0; sz < 2; sz++) {
                let corner = light.position + vec3<f32>(
                    signs[sx] * light.radius,
                    signs[sy] * light.radius,
                    signs[sz] * light.radius,
                );
                let view_pos = params.view * vec4<f32>(corner, 1.0);
                let clip = params.projection * view_pos;
                if clip.w <= 0.0 {
                    continue;
                }
                any_in_front = true;
                let ndc_x = clip.x / clip.w;
                let ndc_y = clip.y / clip.w;
                let t = ndc_to_tile(ndc_x, ndc_y);
                min_t = vec2<i32>(min(min_t.x, t.x), min(min_t.y, t.y));
                max_t = vec2<i32>(max(max_t.x, t.x), max(max_t.y, t.y));
            }
        }
    }
    if !any_in_front {
        return;
    }
    min_t = vec2<i32>(
        max(min_t.x, 0),
        max(min_t.y, 0),
    );
    max_t = vec2<i32>(
        min(max_t.x, i32(params.tile_count_x) - 1),
        min(max_t.y, i32(params.tile_count_y) - 1),
    );
    if min_t.x > max_t.x || min_t.y > max_t.y {
        return;
    }

    let cap = params.max_lights_per_tile;
    for (var ty = min_t.y; ty <= max_t.y; ty++) {
        for (var tx = min_t.x; tx <= max_t.x; tx++) {
            let tile_id = u32(ty) * params.tile_count_x + u32(tx);
            let slot = atomicAdd(&tile_counts[tile_id], 1u);
            if slot < cap {
                let base = tile_id * cap;
                tile_indices[base + slot] = light_idx;
            }
        }
    }
}
";

/// DDGI probe irradiance update compute shader.
///
/// One workgroup per probe (8×8 = 64 threads per probe); each thread
/// blends one octahedral texel of the irradiance map with the new
/// per-direction radiance estimate using the `hysteresis` low-pass
/// filter. Mirrors `ddgi::DdgiVolume::update_probe_irradiance`.
pub const DDGI_UPDATE_COMPUTE_WGSL: &str = r"
struct DdgiParams {
    probe_count: u32,
    irradiance_resolution: u32,
    hysteresis: f32,
    _pad: u32,
};

@group(0) @binding(0) var<uniform> params: DdgiParams;
@group(0) @binding(1) var<storage, read> new_samples: array<f32>;
@group(0) @binding(2) var<storage, read_write> irradiance: array<f32>;

@compute @workgroup_size(8, 8, 1)
fn cs_main(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let probe_idx = wid.x;
    if probe_idx >= params.probe_count {
        return;
    }
    let res = params.irradiance_resolution;
    let x = lid.x;
    let y = lid.y;
    if x >= res || y >= res {
        return;
    }
    let texel_in_probe = y * res + x;
    let texels_per_probe = res * res;
    let channel_base = (probe_idx * texels_per_probe + texel_in_probe) * 3u;
    let alpha = clamp(params.hysteresis, 0.0, 1.0);
    let inv_alpha = 1.0 - alpha;
    for (var c = 0u; c < 3u; c++) {
        let i = channel_base + c;
        let sample = new_samples[i];
        let current = irradiance[i];
        irradiance[i] = sample * alpha + current * inv_alpha;
    }
}
";

/// Image-based lighting (IBL) cubemap lookup helpers in WGSL.
///
/// Provides `sample_irradiance` for diffuse IBL and
/// `sample_radiance_mip` for the GGX split-sum specular IBL. Bind
/// cubemaps as `texture_cube<f32>` on bindings 0 (irradiance) and
/// 1 (prefiltered radiance) along with a linear sampler on binding 2.
pub const IBL_LOOKUP_WGSL: &str = r"
@group(0) @binding(0) var t_irradiance: texture_cube<f32>;
@group(0) @binding(1) var t_radiance: texture_cube<f32>;
@group(0) @binding(2) var s_linear: sampler;

struct IblUniforms {
    radiance_mip_count: u32,
    _pad: vec3<u32>,
};

@group(0) @binding(3) var<uniform> u_ibl: IblUniforms;

fn sample_irradiance(normal: vec3<f32>) -> vec3<f32> {
    return textureSample(t_irradiance, s_linear, normal).rgb;
}

fn sample_radiance_mip(reflect_dir: vec3<f32>, roughness: f32) -> vec3<f32> {
    let max_mip = f32(u_ibl.radiance_mip_count) - 1.0;
    let mip = clamp(roughness, 0.0, 1.0) * max_mip;
    return textureSampleLevel(t_radiance, s_linear, reflect_dir, mip).rgb;
}

@fragment
fn fs_main(@location(0) normal: vec3<f32>) -> @location(0) vec4<f32> {
    // Trivial demonstration entry: returns the diffuse irradiance for a
    // fragment-supplied normal. Real materials combine this with the
    // specular sample using the split-sum BRDF.
    let irr = sample_irradiance(normalize(normal));
    return vec4<f32>(irr, 1.0);
}
";

/// Tiled (Forward+ style) deferred lighting fragment shader.
///
/// Reads a per-tile light index list from a storage buffer and only
/// evaluates the lights that affect the current pixel's tile. The
/// directional-light loop is separate and runs once per pixel because
/// directional lights affect every tile.
///
/// Bind layout:
///
/// | binding | resource | use |
/// |---------|----------|-----|
/// | 0       | `texture_2d<f32>` albedo | `GBuffer` slot 0 |
/// | 1       | `texture_2d<f32>` normal | `GBuffer` slot 1 |
/// | 2       | `texture_2d<f32>` material | `GBuffer` slot 3 |
/// | 3       | `sampler` | linear |
/// | 4       | `uniform` `TileLightingUniforms` | camera + grid params |
/// | 5       | `storage<read>` `array<Light>` | all lights, indexed by `LightRef` |
/// | 6       | `storage<read>` `array<TileEntry>` | per-tile `(offset, count)` |
/// | 7       | `storage<read>` `array<u32>` | flattened tile→light indices |
/// | 8       | `storage<read>` `array<u32>` | directional light indices |
pub const TILED_LIGHTING_FRAGMENT_WGSL: &str = r"
struct Light {
    position: vec3<f32>,
    radius: f32,
    color: vec3<f32>,
    intensity: f32,
    direction: vec3<f32>,
    variant: u32,        // 0 = directional, 1 = point, 2 = spot
};

struct TileEntry {
    offset: u32,
    count: u32,
};

struct TileLightingUniforms {
    camera_pos: vec3<f32>,
    tile_count_x: u32,
    tile_size: u32,
    screen_w: u32,
    screen_h: u32,
    directional_count: u32,
};

@group(0) @binding(0) var t_albedo: texture_2d<f32>;
@group(0) @binding(1) var t_normal: texture_2d<f32>;
@group(0) @binding(2) var t_material: texture_2d<f32>;
@group(0) @binding(3) var s_linear: sampler;
@group(0) @binding(4) var<uniform> u: TileLightingUniforms;
@group(0) @binding(5) var<storage, read> lights: array<Light>;
@group(0) @binding(6) var<storage, read> tile_entries: array<TileEntry>;
@group(0) @binding(7) var<storage, read> tile_light_indices: array<u32>;
@group(0) @binding(8) var<storage, read> directional_indices: array<u32>;

fn shade_point(albedo: vec3<f32>, normal: vec3<f32>, light: Light, world_pos: vec3<f32>) -> vec3<f32> {
    let to_light = light.position - world_pos;
    let dist = length(to_light);
    if dist > light.radius {
        return vec3<f32>(0.0);
    }
    let l = to_light / max(dist, 1e-4);
    let ndotl = max(dot(normal, l), 0.0);
    // Inverse-square falloff softened by radius window.
    let atten = max(0.0, 1.0 - dist / light.radius);
    return albedo * light.color * light.intensity * ndotl * atten;
}

fn shade_directional(albedo: vec3<f32>, normal: vec3<f32>, light: Light) -> vec3<f32> {
    let l = normalize(-light.direction);
    let ndotl = max(dot(normal, l), 0.0);
    return albedo * light.color * light.intensity * ndotl;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let albedo = textureSample(t_albedo, s_linear, uv).rgb;
    let normal_packed = textureSample(t_normal, s_linear, uv).rgb;
    let normal = normalize(normal_packed * 2.0 - 1.0);
    // World position reconstruction is the caller's responsibility — for
    // the tiled shader we work in view-space simplifications: the depth
    // buffer integration lives in the renderer wiring, not this shader
    // skeleton. Pixel-space tile lookup uses the fragment coordinate.
    let pixel = vec2<u32>(
        u32(uv.x * f32(u.screen_w)),
        u32(uv.y * f32(u.screen_h)),
    );
    let tx = pixel.x / max(u.tile_size, 1u);
    let ty = pixel.y / max(u.tile_size, 1u);
    let tile_idx = ty * u.tile_count_x + tx;
    let entry = tile_entries[tile_idx];

    var color = albedo * 0.03; // ambient

    // Per-tile point / spot lights.
    let pixel_world = vec3<f32>(uv.x, uv.y, 0.0); // placeholder world recon
    for (var i: u32 = 0u; i < entry.count; i++) {
        let light_idx = tile_light_indices[entry.offset + i];
        let light = lights[light_idx];
        color += shade_point(albedo, normal, light, pixel_world);
    }

    // Directional lights affect every fragment.
    for (var i: u32 = 0u; i < u.directional_count; i++) {
        let light_idx = directional_indices[i];
        let light = lights[light_idx];
        color += shade_directional(albedo, normal, light);
    }

    return vec4<f32>(color, 1.0);
}
";

/// Deferred decal vertex shader.
///
/// Takes a unit cube `[-1, 1]^3` as input (8 vertices, 36 indices) and
/// transforms it by the decal's world matrix. The fragment shader then
/// reconstructs the world position of the underlying `GBuffer` pixel from
/// the depth buffer and discards fragments outside the OBB.
pub const DECAL_VERTEX_WGSL: &str = r"
struct VertexInput {
    @location(0) position: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) screen_uv: vec2<f32>,
};

struct DecalUniforms {
    world_matrix: mat4x4<f32>,
    inv_world_matrix: mat4x4<f32>,
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    inv_view_projection: mat4x4<f32>,
    color_opacity: vec4<f32>,
    blend_layer: vec4<u32>,
};

@group(0) @binding(0) var<uniform> u: DecalUniforms;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = u.world_matrix * vec4<f32>(in.position, 1.0);
    let clip = u.projection * u.view * world_pos;
    out.clip_position = clip;
    // Screen UV in [0, 1], y flipped to match WGPU's top-left origin.
    let ndc = clip.xy / clip.w;
    out.screen_uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    return out;
}
";

/// Deferred decal fragment shader.
///
/// Reads the depth buffer to reconstruct world position, transforms back
/// into projector-local space, discards fragments whose `|local.xyz| > 1`,
/// samples the decal albedo texture using `local.xy` and blends onto the
/// `GBuffer` per `blend_layer.x` (= `DecalBlendMode::shader_id`).
pub const DECAL_FRAGMENT_WGSL: &str = r"
@group(0) @binding(1) var t_depth: texture_depth_2d;
@group(0) @binding(2) var s_depth: sampler;
@group(0) @binding(3) var t_decal_albedo: texture_2d<f32>;
@group(0) @binding(4) var s_decal: sampler;

struct DecalUniforms {
    world_matrix: mat4x4<f32>,
    inv_world_matrix: mat4x4<f32>,
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    inv_view_projection: mat4x4<f32>,
    color_opacity: vec4<f32>,
    blend_layer: vec4<u32>,
};

@group(0) @binding(0) var<uniform> u: DecalUniforms;

struct DecalOutput {
    @location(0) albedo: vec4<f32>,
};

@fragment
fn fs_main(@location(0) screen_uv: vec2<f32>) -> DecalOutput {
    // Reconstruct world position from depth.
    let depth = textureSample(t_depth, s_depth, screen_uv);
    // NDC Z in WGPU is [0, 1]; convert UV back to NDC XY and reproject.
    let ndc = vec3<f32>(
        screen_uv.x * 2.0 - 1.0,
        1.0 - screen_uv.y * 2.0,
        depth,
    );
    let world_h = u.inv_view_projection * vec4<f32>(ndc, 1.0);
    let world = world_h.xyz / world_h.w;

    // World → projector local. Outside the unit cube → discard.
    let local = (u.inv_world_matrix * vec4<f32>(world, 1.0)).xyz;
    let bounds = abs(local) - vec3<f32>(1.0, 1.0, 1.0);
    if bounds.x > 0.0 || bounds.y > 0.0 || bounds.z > 0.0 {
        discard;
    }

    // Local XY ∈ [-1, 1] → UV ∈ [0, 1]. Y flipped for texture convention.
    let decal_uv = vec2<f32>(local.x * 0.5 + 0.5, 0.5 - local.y * 0.5);
    let sample = textureSample(t_decal_albedo, s_decal, decal_uv);
    let tint = u.color_opacity;
    let alpha = sample.a * tint.a;

    var out: DecalOutput;
    let blend_id = u.blend_layer.x;
    if blend_id == 0u {
        // AlphaBlend: premultiplied output, fixed-function blend dst rgb = src + (1-src.a)*dst
        out.albedo = vec4<f32>(sample.rgb * tint.rgb * alpha, alpha);
    } else if blend_id == 1u {
        // Multiply: lerp(white, sample*tint, alpha), set alpha to 1 so dst rgb *= src.rgb
        let multiplied = mix(vec3<f32>(1.0, 1.0, 1.0), sample.rgb * tint.rgb, alpha);
        out.albedo = vec4<f32>(multiplied, 1.0);
    } else {
        // Additive: dst rgb += src.rgb * alpha, dst alpha unchanged
        out.albedo = vec4<f32>(sample.rgb * tint.rgb * alpha, 0.0);
    }
    return out;
}
";

/// Deferred lighting pass fragment shader.
pub const DEFERRED_LIGHTING_FRAGMENT_WGSL: &str = r"
@group(0) @binding(0) var t_albedo: texture_2d<f32>;
@group(0) @binding(1) var t_normal: texture_2d<f32>;
@group(0) @binding(2) var t_material: texture_2d<f32>;
@group(0) @binding(3) var s_linear: sampler;

struct Light {
    position: vec3<f32>,
    _pad0: f32,
    color: vec3<f32>,
    intensity: f32,
};

struct LightUniforms {
    camera_pos: vec3<f32>,
    light_count: u32,
    lights: array<Light, 16>,
};

@group(1) @binding(0) var<uniform> lu: LightUniforms;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let albedo = textureSample(t_albedo, s_linear, uv).rgb;
    let normal = textureSample(t_normal, s_linear, uv).rgb * 2.0 - 1.0;
    let material = textureSample(t_material, s_linear, uv);
    let roughness = material.g;

    var color = albedo * 0.03; // ambient
    for (var i: u32 = 0u; i < lu.light_count; i++) {
        let l = lu.lights[i];
        let light_dir = normalize(l.position - vec3<f32>(uv.x, uv.y, 0.0));
        let diff = max(dot(normal, light_dir), 0.0);
        color += albedo * l.color * diff * l.intensity;
    }
    return vec4<f32>(color, 1.0);
}
";

// ---------------------------------------------------------------------------
// Preloaded cache
// ---------------------------------------------------------------------------

/// Returns a `ShaderCache` pre-loaded with all built-in shaders.
#[must_use]
pub fn builtin_shader_cache() -> ShaderCache {
    let mut cache = ShaderCache::new();
    cache.add(ShaderSource::new(
        "gbuffer_vertex",
        GBUFFER_VERTEX_WGSL,
        ShaderStage::Vertex,
    ));
    cache.add(ShaderSource::new(
        "gbuffer_fragment",
        GBUFFER_FRAGMENT_WGSL,
        ShaderStage::Fragment,
    ));
    cache.add(ShaderSource::new(
        "sdf_raymarch",
        SDF_RAYMARCH_FRAGMENT_WGSL,
        ShaderStage::Fragment,
    ));
    cache.add(ShaderSource::new(
        "fullscreen_vertex",
        FULLSCREEN_VERTEX_WGSL,
        ShaderStage::Vertex,
    ));
    cache.add(ShaderSource::new(
        "deferred_lighting",
        DEFERRED_LIGHTING_FRAGMENT_WGSL,
        ShaderStage::Fragment,
    ));
    cache.add(ShaderSource::new(
        "decal_vertex",
        DECAL_VERTEX_WGSL,
        ShaderStage::Vertex,
    ));
    cache.add(ShaderSource::new(
        "decal_fragment",
        DECAL_FRAGMENT_WGSL,
        ShaderStage::Fragment,
    ));
    cache.add(ShaderSource::new(
        "tiled_lighting_fragment",
        TILED_LIGHTING_FRAGMENT_WGSL,
        ShaderStage::Fragment,
    ));
    cache.add(ShaderSource::new(
        "ibl_lookup",
        IBL_LOOKUP_WGSL,
        ShaderStage::Fragment,
    ));
    cache.add(ShaderSource::new(
        "cubemap_sky_fragment",
        CUBEMAP_SKY_FRAGMENT_WGSL,
        ShaderStage::Fragment,
    ));
    cache.add(ShaderSource::new(
        "gpu_bvh_refit_compute",
        GPU_BVH_REFIT_COMPUTE_WGSL,
        ShaderStage::Compute,
    ));
    cache.add(ShaderSource::new(
        "gpu_bvh_interior_refit_compute",
        GPU_BVH_INTERIOR_REFIT_COMPUTE_WGSL,
        ShaderStage::Compute,
    ));
    cache.add(ShaderSource::new(
        "light_culling_compute",
        LIGHT_CULLING_COMPUTE_WGSL,
        ShaderStage::Compute,
    ));
    cache.add(ShaderSource::new(
        "ddgi_update_compute",
        DDGI_UPDATE_COMPUTE_WGSL,
        ShaderStage::Compute,
    ));
    cache.add(ShaderSource::new(
        "lut_postprocess",
        crate::lut_postprocess::LUT_POSTPROCESS_WGSL,
        ShaderStage::Fragment,
    ));
    cache
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_source_new() {
        let s = ShaderSource::new("test", "@vertex fn vs_main() {}", ShaderStage::Vertex);
        assert_eq!(s.name, "test");
        assert!(s.has_entry_point());
    }

    #[test]
    fn shader_no_entry_point() {
        let s = ShaderSource::new("bad", "fn helper() {}", ShaderStage::Vertex);
        assert!(!s.has_entry_point());
    }

    #[test]
    fn shader_fragment_entry() {
        let s = ShaderSource::new("frag", "@fragment fn fs_main() {}", ShaderStage::Fragment);
        assert!(s.has_entry_point());
    }

    #[test]
    fn shader_compute_entry() {
        let s = ShaderSource::new("comp", "@compute fn cs_main() {}", ShaderStage::Compute);
        assert!(s.has_entry_point());
    }

    #[test]
    fn shader_line_count() {
        let s = ShaderSource::new("test", "line1\nline2\nline3", ShaderStage::Vertex);
        assert_eq!(s.line_count(), 3);
    }

    #[test]
    fn cache_add_get() {
        let mut cache = ShaderCache::new();
        cache.add(ShaderSource::new("a", "code", ShaderStage::Vertex));
        assert!(cache.get("a").is_some());
        assert!(cache.get("b").is_none());
    }

    #[test]
    fn cache_remove() {
        let mut cache = ShaderCache::new();
        cache.add(ShaderSource::new("a", "code", ShaderStage::Vertex));
        cache.remove("a");
        assert_eq!(cache.count(), 0);
    }

    #[test]
    fn cache_names() {
        let mut cache = ShaderCache::new();
        cache.add(ShaderSource::new("x", "", ShaderStage::Vertex));
        cache.add(ShaderSource::new("y", "", ShaderStage::Fragment));
        let names = cache.names();
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn builtin_cache_has_all() {
        let cache = builtin_shader_cache();
        assert_eq!(cache.count(), 15);
        assert!(cache.get("light_culling_compute").is_some());
        assert!(cache.get("ddgi_update_compute").is_some());
        assert!(cache.get("cubemap_sky_fragment").is_some());
        assert!(cache.get("gpu_bvh_refit_compute").is_some());
        assert!(cache.get("gpu_bvh_interior_refit_compute").is_some());
        assert!(cache.get("ibl_lookup").is_some());
        assert!(cache.get("gbuffer_vertex").is_some());
        assert!(cache.get("gbuffer_fragment").is_some());
        assert!(cache.get("sdf_raymarch").is_some());
        assert!(cache.get("fullscreen_vertex").is_some());
        assert!(cache.get("deferred_lighting").is_some());
        assert!(cache.get("decal_vertex").is_some());
        assert!(cache.get("decal_fragment").is_some());
        assert!(cache.get("tiled_lighting_fragment").is_some());
        assert!(cache.get("lut_postprocess").is_some());
    }

    #[test]
    fn decal_vertex_has_uniforms_and_entry() {
        assert!(DECAL_VERTEX_WGSL.contains("DecalUniforms"));
        assert!(DECAL_VERTEX_WGSL.contains("inv_world_matrix"));
        assert!(DECAL_VERTEX_WGSL.contains("@vertex"));
    }

    #[test]
    fn decal_fragment_has_blend_branches() {
        // Blend mode IDs must match decal::DecalBlendMode::shader_id().
        assert!(DECAL_FRAGMENT_WGSL.contains("blend_id == 0u"));
        assert!(DECAL_FRAGMENT_WGSL.contains("blend_id == 1u"));
        assert!(DECAL_FRAGMENT_WGSL.contains("discard"));
        assert!(DECAL_FRAGMENT_WGSL.contains("@fragment"));
    }

    #[test]
    fn decal_shaders_pass_naga_validation() {
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        for (label, src) in [
            ("decal_vertex", DECAL_VERTEX_WGSL),
            ("decal_fragment", DECAL_FRAGMENT_WGSL),
        ] {
            let module = naga::front::wgsl::parse_str(src)
                .unwrap_or_else(|e| panic!("{label} WGSL parse failed: {e:?}"));
            let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
            validator
                .validate(&module)
                .unwrap_or_else(|e| panic!("{label} naga validation failed: {e:?}"));
        }
    }

    #[test]
    fn gpu_bvh_interior_refit_shader_passes_naga_validation() {
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = naga::front::wgsl::parse_str(GPU_BVH_INTERIOR_REFIT_COMPUTE_WGSL)
            .unwrap_or_else(|e| panic!("gpu_bvh_interior_refit_compute parse failed: {e:?}"));
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator
            .validate(&module)
            .unwrap_or_else(|e| panic!("gpu_bvh_interior_refit_compute validation failed: {e:?}"));
    }

    #[test]
    fn gpu_bvh_refit_shader_passes_naga_validation() {
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = naga::front::wgsl::parse_str(GPU_BVH_REFIT_COMPUTE_WGSL)
            .unwrap_or_else(|e| panic!("gpu_bvh_refit_compute WGSL parse failed: {e:?}"));
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator
            .validate(&module)
            .unwrap_or_else(|e| panic!("gpu_bvh_refit_compute naga validation failed: {e:?}"));
    }

    #[test]
    fn cubemap_sky_shader_passes_naga_validation() {
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = naga::front::wgsl::parse_str(CUBEMAP_SKY_FRAGMENT_WGSL)
            .unwrap_or_else(|e| panic!("cubemap_sky_fragment WGSL parse failed: {e:?}"));
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator
            .validate(&module)
            .unwrap_or_else(|e| panic!("cubemap_sky_fragment naga validation failed: {e:?}"));
    }

    #[test]
    fn compute_shaders_pass_naga_validation() {
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        for (label, src) in [
            ("light_culling_compute", LIGHT_CULLING_COMPUTE_WGSL),
            ("ddgi_update_compute", DDGI_UPDATE_COMPUTE_WGSL),
        ] {
            let module = naga::front::wgsl::parse_str(src)
                .unwrap_or_else(|e| panic!("{label} WGSL parse failed: {e:?}"));
            let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
            validator
                .validate(&module)
                .unwrap_or_else(|e| panic!("{label} naga validation failed: {e:?}"));
        }
    }

    #[test]
    fn ibl_lookup_shader_passes_naga_validation() {
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = naga::front::wgsl::parse_str(IBL_LOOKUP_WGSL)
            .unwrap_or_else(|e| panic!("ibl_lookup WGSL parse failed: {e:?}"));
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator
            .validate(&module)
            .unwrap_or_else(|e| panic!("ibl_lookup naga validation failed: {e:?}"));
    }

    #[test]
    fn tiled_lighting_shader_passes_naga_validation() {
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = naga::front::wgsl::parse_str(TILED_LIGHTING_FRAGMENT_WGSL)
            .unwrap_or_else(|e| panic!("tiled_lighting WGSL parse failed: {e:?}"));
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator
            .validate(&module)
            .unwrap_or_else(|e| panic!("tiled_lighting naga validation failed: {e:?}"));
    }

    #[test]
    fn builtin_shaders_have_entry_points() {
        let cache = builtin_shader_cache();
        for name in cache.names() {
            let shader = cache.get(name).unwrap();
            assert!(
                shader.has_entry_point(),
                "Shader {name} missing entry point"
            );
        }
    }

    #[test]
    fn gbuffer_vertex_has_uniforms() {
        assert!(GBUFFER_VERTEX_WGSL.contains("Uniforms"));
        assert!(GBUFFER_VERTEX_WGSL.contains("model"));
    }

    #[test]
    fn sdf_raymarch_has_sphere() {
        assert!(SDF_RAYMARCH_FRAGMENT_WGSL.contains("sdf_sphere"));
    }

    #[test]
    fn deferred_lighting_has_lights() {
        assert!(DEFERRED_LIGHTING_FRAGMENT_WGSL.contains("Light"));
        assert!(DEFERRED_LIGHTING_FRAGMENT_WGSL.contains("light_count"));
    }

    #[test]
    fn fullscreen_vertex_has_positions() {
        assert!(FULLSCREEN_VERTEX_WGSL.contains("positions"));
    }

    #[test]
    fn shader_cache_default() {
        let cache = ShaderCache::default();
        assert_eq!(cache.count(), 0);
    }
}
