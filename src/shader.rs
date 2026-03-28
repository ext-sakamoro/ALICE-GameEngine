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
        assert_eq!(cache.count(), 5);
        assert!(cache.get("gbuffer_vertex").is_some());
        assert!(cache.get("gbuffer_fragment").is_some());
        assert!(cache.get("sdf_raymarch").is_some());
        assert!(cache.get("fullscreen_vertex").is_some());
        assert!(cache.get("deferred_lighting").is_some());
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
