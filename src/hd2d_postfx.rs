//! HD-2D billboard sprites + screen-space post-process (SSGI + SMAA) — code
//! definitions + WGSL templates only. **GPU verification is deferred** to a
//! windowed harness session; the WGSL is included verbatim so a renderer
//! integration step can string-feed it to `wgpu::ShaderModule`.
//!
//! ## Pieces
//!
//! - [`Sprite3D`] — billboard sprite with world-space anchor, used to mix
//!   2D pixel art with 3D SDF/mesh scenes (Octopath Traveler style)
//! - [`SpriteMode`] — Unlit / Lit / Shaded (PBR blend) selector
//! - [`hd2d_sprite_wgsl`] / [`ssgi_wgsl`] / [`smaa_wgsl`] — WGSL source
//!   templates returned as `&'static str`
//! - [`PostFxConfig`] — runtime tuning knobs (SSGI intensity / SMAA edge
//!   threshold / etc.)

use crate::math::{Color, Vec3};

/// Lighting / shading mode for a [`Sprite3D`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpriteMode {
    /// No lighting — sprite is shown at full color (UI, particles).
    #[default]
    Unlit,
    /// Single-tap Lambertian against the scene light direction.
    Lit,
    /// PBR-style: sprite participates in deferred lighting, can receive
    /// shadows / SSGI / fog.
    Shaded,
}

/// A 3D-anchored billboard sprite. Position is world-space; the renderer
/// orients the quad toward the active camera each frame.
#[derive(Debug, Clone)]
pub struct Sprite3D {
    pub position: Vec3,
    pub size: f32,
    pub texture_id: u32,
    pub tint: Color,
    pub mode: SpriteMode,
    /// `0.0..1.0` — how much the sprite "leans" toward the camera vs
    /// staying world-axis-aligned. `1.0` is full billboard, `0.0` is a
    /// flat decal.
    pub billboard_bias: f32,
}

impl Sprite3D {
    #[must_use]
    pub const fn new(position: Vec3, size: f32, texture_id: u32) -> Self {
        Self {
            position,
            size,
            texture_id,
            tint: Color::WHITE,
            mode: SpriteMode::Unlit,
            billboard_bias: 1.0,
        }
    }

    #[must_use]
    pub const fn with_mode(mut self, mode: SpriteMode) -> Self {
        self.mode = mode;
        self
    }
}

/// Configuration for the HD-2D / post-process passes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PostFxConfig {
    /// SSGI bounce intensity, `0.0` (off) .. `2.0` (heavy).
    pub ssgi_intensity: f32,
    /// SSGI sample radius in world units (small = local AO, large = bounce
    /// from distant geometry).
    pub ssgi_radius: f32,
    /// SMAA edge-detection threshold (`0.0..0.5`). Lower catches more edges.
    pub smaa_threshold: f32,
    /// Pixel-art sharpness: `1.0` = nearest-neighbor, `0.0` = bilinear.
    /// HD-2D blend looks best around `0.85` — crisp sprites with anti-aliased
    /// edges.
    pub pixel_art_sharpness: f32,
}

impl Default for PostFxConfig {
    fn default() -> Self {
        Self {
            ssgi_intensity: 0.7,
            ssgi_radius: 2.5,
            smaa_threshold: 0.1,
            pixel_art_sharpness: 0.85,
        }
    }
}

/// HD-2D pbr-sprite WGSL — a billboard fragment shader that samples a
/// pixel-art texture with controllable sharpness, then mixes deferred
/// lighting in based on [`SpriteMode`] (passed as a uniform).
#[must_use]
pub const fn hd2d_sprite_wgsl() -> &'static str {
    r"
// HD-2D pbr-sprite — billboard sampler + lit/shaded mode mixer.
//
// Bindings (assumed by the renderer):
//   group(0) binding(0): MvpUniforms
//   group(1) binding(0): texture_2d<f32>  (the sprite atlas)
//   group(1) binding(1): sampler          (anisotropic + linear)
//   group(2) binding(0): SpriteUniforms { mode: u32, tint: vec4, sharpness: f32, ... }

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) world_pos: vec3<f32>,
};

struct SpriteUniforms {
    tint: vec4<f32>,
    mode: u32,
    sharpness: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;
@group(2) @binding(0) var<uniform> sprite: SpriteUniforms;

// Box-filter-then-quantize for pixel-art sharpness.
fn sample_sprite(uv: vec2<f32>, sharpness: f32) -> vec4<f32> {
    let tex_size = vec2<f32>(textureDimensions(atlas, 0));
    let texel = uv * tex_size;
    let frac = fract(texel);
    let bias = clamp((frac - vec2<f32>(0.5)) * (1.0 + sharpness * 4.0)
                   + vec2<f32>(0.5), vec2<f32>(0.0), vec2<f32>(1.0));
    let snapped_uv = (floor(texel) + bias) / tex_size;
    return textureSample(atlas, atlas_sampler, snapped_uv);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var color = sample_sprite(in.uv, sprite.sharpness) * sprite.tint;
    if (color.a < 0.01) { discard; }

    // Mode 0 = Unlit, 1 = Lit (Lambert), 2 = Shaded (handled by deferred pass)
    if (sprite.mode == 1u) {
        // Cheap directional Lambert — sun pointing roughly from above.
        let n = normalize(in.world_normal);
        let l = normalize(vec3<f32>(0.3, 0.9, 0.2));
        let lambert = max(dot(n, l), 0.0);
        let ambient = 0.25;
        color.r *= ambient + lambert * 0.75;
        color.g *= ambient + lambert * 0.75;
        color.b *= ambient + lambert * 0.75;
    }
    // Mode 2: passthrough — deferred shading pass picks this up via GBuffer.
    return color;
}
"
}

/// SSGI screen-space global illumination WGSL — single-bounce, sphere-tap
/// sampling against the depth + normal `GBuffer`. Returns colored AO/bounce
/// to be additively blended into the lighting buffer.
#[must_use]
pub const fn ssgi_wgsl() -> &'static str {
    r"
// SSGI — screen-space global illumination, single bounce.
//
// Bindings:
//   group(0) binding(0): texture_2d<f32>  depth
//   group(0) binding(1): texture_2d<f32>  normal (world-space)
//   group(0) binding(2): texture_2d<f32>  albedo (lit prev frame)
//   group(0) binding(3): sampler          (clamp-to-edge, linear)
//   group(1) binding(0): SsgiUniforms { intensity, radius, frame_index, _pad }

struct SsgiUniforms {
    intensity: f32,
    radius: f32,
    frame_index: u32,
    _pad: f32,
};

@group(0) @binding(0) var gb_depth: texture_2d<f32>;
@group(0) @binding(1) var gb_normal: texture_2d<f32>;
@group(0) @binding(2) var lit_prev: texture_2d<f32>;
@group(0) @binding(3) var smp: sampler;
@group(1) @binding(0) var<uniform> u: SsgiUniforms;

const PI: f32 = 3.14159265;
const SAMPLES: u32 = 16u;

// Cheap hash → pseudorandom in [0,1].
fn rand(uv: vec2<f32>, seed: u32) -> f32 {
    let h = sin(dot(uv, vec2<f32>(12.9898, 78.233))
              + f32(seed) * 0.5) * 43758.547;
    return fract(h);
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let dim = vec2<f32>(textureDimensions(gb_depth));
    let uv = frag.xy / dim;
    let center_depth = textureSample(gb_depth, smp, uv).r;
    let center_n = normalize(textureSample(gb_normal, smp, uv).xyz * 2.0 - 1.0);
    if (center_depth >= 0.999) {
        return vec4<f32>(0.0);
    }

    var acc = vec3<f32>(0.0);
    var weight_sum = 0.0;
    for (var i: u32 = 0u; i < SAMPLES; i = i + 1u) {
        let r0 = rand(uv, u.frame_index * SAMPLES + i);
        let r1 = rand(uv + vec2<f32>(0.131, 0.713), u.frame_index * SAMPLES + i);
        let theta = r0 * 2.0 * PI;
        let radius = u.radius * sqrt(r1);
        let offset = vec2<f32>(cos(theta), sin(theta)) * (radius / dim.x);
        let sample_uv = clamp(uv + offset, vec2<f32>(0.0), vec2<f32>(1.0));

        let sd = textureSample(gb_depth, smp, sample_uv).r;
        let dz = abs(sd - center_depth);
        if (dz > 0.05 || sd >= 0.999) { continue; }

        let sn = normalize(textureSample(gb_normal, smp, sample_uv).xyz * 2.0 - 1.0);
        let bounce = textureSample(lit_prev, smp, sample_uv).rgb;
        let n_dot = max(dot(center_n, sn), 0.0);

        acc = acc + bounce * n_dot;
        weight_sum = weight_sum + 1.0;
    }
    if (weight_sum > 0.0) {
        acc = acc / weight_sum;
    }
    return vec4<f32>(acc * u.intensity, 1.0);
}
"
}

/// SMAA (Subpixel Morphological Anti-Aliasing) — edge-detection pass.
/// This is the first of SMAA's three passes (edge detection); the
/// blending pass and neighborhood blending pass are separate fragments
/// the renderer can chain after this one.
#[must_use]
pub const fn smaa_wgsl() -> &'static str {
    r"
// SMAA edge detection pass — luma-based, classic SMAA.
//
// Bindings:
//   group(0) binding(0): texture_2d<f32>  color
//   group(0) binding(1): sampler          (clamp-to-edge, linear)
//   group(1) binding(0): SmaaUniforms { threshold, contrast_factor, _pad, _pad2 }

struct SmaaUniforms {
    threshold: f32,
    contrast_factor: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0) var color_tex: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
@group(1) @binding(0) var<uniform> u: SmaaUniforms;

fn luma(rgb: vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let dim = vec2<f32>(textureDimensions(color_tex));
    let uv = frag.xy / dim;
    let invd = 1.0 / dim;

    let cc = luma(textureSample(color_tex, smp, uv).rgb);
    let cl = luma(textureSample(color_tex, smp, uv + vec2<f32>(-invd.x, 0.0)).rgb);
    let cr = luma(textureSample(color_tex, smp, uv + vec2<f32>(invd.x, 0.0)).rgb);
    let cu = luma(textureSample(color_tex, smp, uv + vec2<f32>(0.0, -invd.y)).rgb);
    let cd = luma(textureSample(color_tex, smp, uv + vec2<f32>(0.0, invd.y)).rgb);

    let edge_h = abs(cl - cc) + abs(cr - cc);
    let edge_v = abs(cu - cc) + abs(cd - cc);
    let local_contrast = max(abs(cl - cr), abs(cu - cd)) * u.contrast_factor;
    let final_thr = max(u.threshold, local_contrast);

    let h = step(final_thr, edge_h);
    let v = step(final_thr, edge_v);
    return vec4<f32>(h, v, 0.0, 1.0);
}
"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite3d_constructor_defaults_to_unlit() {
        let s = Sprite3D::new(Vec3::ZERO, 1.0, 42);
        assert_eq!(s.mode, SpriteMode::Unlit);
        assert_eq!(s.texture_id, 42);
        assert!((s.billboard_bias - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn sprite3d_with_mode_builder() {
        let s = Sprite3D::new(Vec3::ZERO, 1.0, 0).with_mode(SpriteMode::Shaded);
        assert_eq!(s.mode, SpriteMode::Shaded);
    }

    #[test]
    fn postfx_config_default_is_balanced() {
        let cfg = PostFxConfig::default();
        assert!(cfg.ssgi_intensity > 0.0 && cfg.ssgi_intensity <= 2.0);
        assert!(cfg.smaa_threshold > 0.0 && cfg.smaa_threshold < 0.5);
        assert!(cfg.pixel_art_sharpness >= 0.0 && cfg.pixel_art_sharpness <= 1.0);
    }

    #[test]
    fn hd2d_sprite_wgsl_contains_required_bindings() {
        let src = hd2d_sprite_wgsl();
        assert!(src.contains("@fragment"));
        assert!(src.contains("textureSample"));
        assert!(src.contains("SpriteUniforms"));
    }

    #[test]
    fn ssgi_wgsl_contains_sampling_loop() {
        let src = ssgi_wgsl();
        assert!(src.contains("SAMPLES"));
        assert!(src.contains("gb_depth"));
        assert!(src.contains("gb_normal"));
    }

    #[test]
    fn smaa_wgsl_contains_edge_detection() {
        let src = smaa_wgsl();
        assert!(src.contains("edge_h"));
        assert!(src.contains("edge_v"));
        assert!(src.contains("luma"));
    }

    #[test]
    fn wgsl_sources_are_non_empty() {
        assert!(hd2d_sprite_wgsl().len() > 100);
        assert!(ssgi_wgsl().len() > 100);
        assert!(smaa_wgsl().len() > 100);
    }

    // naga front-end validation — parses the WGSL into a `Module` and
    // validates type/binding correctness. Catches typos / mismatched
    // bindings / undefined symbols without needing a GPU.
    fn parse_wgsl(src: &str) -> Result<naga::Module, String> {
        naga::front::wgsl::parse_str(src).map_err(|e| format!("{e:?}"))
    }

    fn validate_module(m: &naga::Module) -> Result<(), String> {
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(m)
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
    }

    #[test]
    fn hd2d_sprite_wgsl_parses_and_validates() {
        let m = parse_wgsl(hd2d_sprite_wgsl()).expect("parse hd2d_sprite_wgsl");
        validate_module(&m).expect("validate hd2d_sprite_wgsl");
    }

    #[test]
    fn ssgi_wgsl_parses_and_validates() {
        let m = parse_wgsl(ssgi_wgsl()).expect("parse ssgi_wgsl");
        validate_module(&m).expect("validate ssgi_wgsl");
    }

    #[test]
    fn smaa_wgsl_parses_and_validates() {
        let m = parse_wgsl(smaa_wgsl()).expect("parse smaa_wgsl");
        validate_module(&m).expect("validate smaa_wgsl");
    }

    #[test]
    fn sprite_modes_distinct() {
        assert_ne!(SpriteMode::Unlit, SpriteMode::Lit);
        assert_ne!(SpriteMode::Lit, SpriteMode::Shaded);
    }

    // GPU smoke tests — create a wgpu Device on the host backend
    // (Metal on macOS, Vulkan elsewhere) and load each WGSL as a
    // ShaderModule. Catches backend-specific issues naga's generic
    // validator misses. `#[ignore]` by default because some CI runners
    // lack a usable adapter; run locally with
    // `cargo test --features gpu -- --ignored gpu`.
    #[cfg(feature = "gpu")]
    fn smoke_load_shader(wgsl: &str, label: &str) -> Result<(), String> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| format!("no adapter: {e:?}"))?;
        let (device, _queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some(label),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            }))
            .map_err(|e| format!("device: {e:?}"))?;
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(wgsl.into()),
        });
        let err = pollster::block_on(device.pop_error_scope());
        match err {
            Some(e) => Err(format!("{label} GPU validation: {e:?}")),
            None => Ok(()),
        }
    }

    #[cfg(feature = "gpu")]
    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn hd2d_sprite_wgsl_loads_on_gpu() {
        smoke_load_shader(hd2d_sprite_wgsl(), "hd2d_sprite").unwrap();
    }

    #[cfg(feature = "gpu")]
    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn ssgi_wgsl_loads_on_gpu() {
        smoke_load_shader(ssgi_wgsl(), "ssgi").unwrap();
    }

    #[cfg(feature = "gpu")]
    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn smaa_wgsl_loads_on_gpu() {
        smoke_load_shader(smaa_wgsl(), "smaa").unwrap();
    }

    /// End-to-end offscreen triangle draw — pipeline + render pass +
    /// texture readback. Confirms the green our fragment shader emits
    /// lands on the centre pixel.
    #[cfg(feature = "gpu")]
    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn offscreen_triangle_draw() {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("offscreen"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .expect("device");

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("triangle"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
                @vertex
                fn vs_main(@builtin(vertex_index) vid: u32) -> @builtin(position) vec4<f32> {
                    var p = array<vec2<f32>, 3>(
                        vec2<f32>(-0.7, -0.7),
                        vec2<f32>( 0.7, -0.7),
                        vec2<f32>( 0.0,  0.7),
                    );
                    return vec4<f32>(p[vid], 0.0, 1.0);
                }
                @fragment
                fn fs_main() -> @location(0) vec4<f32> {
                    return vec4<f32>(0.2, 0.8, 0.3, 1.0);
                }
            "#
                .into(),
            ),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("triangle"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("target"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());

        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("triangle"),
        });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&pipeline);
            pass.draw(0..3, 0..1);
        }

        let buf_size = 64 * 64 * 4;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: buf_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(64 * 4),
                    rows_per_image: Some(64),
                },
            },
            wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(enc.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::Wait).unwrap();
        let data = slice.get_mapped_range();

        let off = (32 * 64 + 32) * 4;
        let r = data[off];
        let g = data[off + 1];
        let b = data[off + 2];
        drop(data);
        readback.unmap();
        assert!(
            g > r && g > b,
            "expected green-dominant pixel, got ({r}, {g}, {b})"
        );
    }
}
