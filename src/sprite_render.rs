//! Real wgpu render path for [`crate::hd2d_postfx::Sprite3D`]. Builds a
//! complete vertex + fragment pipeline (no GBuffer dependency), uploads a
//! tinted sprite, draws into an offscreen RGBA8 texture, and returns the
//! readback bytes. Intended both as the production starting point for
//! HD-2D rendering and as a deterministic CPU-side smoke test that confirms
//! the rendering path end-to-end on a real GPU.

use crate::hd2d_postfx::Sprite3D;

/// Complete WGSL containing a vertex stage (builds a unit quad billboard)
/// and a fragment stage (textured + tinted). Designed to compile under
/// `Limits::downlevel_defaults()`.
#[must_use]
pub const fn sprite3d_pipeline_wgsl() -> &'static str {
    r"
struct Frame {
    view_proj: mat4x4<f32>,
    sprite_pos: vec4<f32>, // xyz = world position, w = size
    tint:       vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    // Two-triangle strip for a billboard quad in NDC, then translated into
    // world-space via sprite_pos.xyz.
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>(-0.5,  0.5),
        vec2<f32>(-0.5,  0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>( 0.5,  0.5),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );
    let p = positions[vid] * frame.sprite_pos.w;
    let world = vec4<f32>(p.x + frame.sprite_pos.x,
                          p.y + frame.sprite_pos.y,
                          frame.sprite_pos.z,
                          1.0);

    var out: VsOut;
    out.clip_pos = frame.view_proj * world;
    out.uv = uvs[vid];
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let tex_color = textureSample(atlas, atlas_sampler, in.uv);
    return tex_color * frame.tint;
}
"
}

/// Errors that can come out of [`render_sprite_to_rgba8`].
#[derive(Debug)]
pub enum SpriteRenderError {
    NoAdapter,
    NoDevice(String),
    PixelLayout(String),
}

impl std::fmt::Display for SpriteRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAdapter => f.write_str("no wgpu adapter available"),
            Self::NoDevice(msg) => write!(f, "device init: {msg}"),
            Self::PixelLayout(msg) => write!(f, "pixel layout: {msg}"),
        }
    }
}

impl std::error::Error for SpriteRenderError {}

/// Render the given [`Sprite3D`] into a `width × height` RGBA8 buffer
/// using a fresh wgpu device. `atlas_rgba8` is the sprite texture
/// (`atlas_width * atlas_height * 4` bytes).
///
/// The view-projection matrix is hard-coded to an orthographic camera
/// that maps world `(-1, -1) .. (1, 1)` to the viewport — adequate for a
/// 2D / HD-2D billboard validation pass.
///
/// # Errors
/// Returns [`SpriteRenderError`] when the wgpu adapter / device fail to
/// initialise, or when the requested viewport size produces an invalid
/// row layout.
#[cfg(feature = "gpu")]
pub fn render_sprite_to_rgba8(
    sprite: &Sprite3D,
    atlas_rgba8: &[u8],
    atlas_width: u32,
    atlas_height: u32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, SpriteRenderError> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .map_err(|_| SpriteRenderError::NoAdapter)?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("sprite_render"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
    }))
    .map_err(|e| SpriteRenderError::NoDevice(format!("{e:?}")))?;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sprite3d"),
        source: wgpu::ShaderSource::Wgsl(sprite3d_pipeline_wgsl().into()),
    });

    // Frame uniform layout matches the WGSL struct ordering.
    let view_proj = orthographic_matrix(-1.5, 1.5, -1.5, 1.5, -1.0, 1.0);
    let mut frame_bytes = Vec::with_capacity(96);
    for v in view_proj {
        frame_bytes.extend_from_slice(&v.to_le_bytes());
    }
    for v in [
        sprite.position.x(),
        sprite.position.y(),
        sprite.position.z(),
        sprite.size,
    ] {
        frame_bytes.extend_from_slice(&v.to_le_bytes());
    }
    let tint = [sprite.tint.r, sprite.tint.g, sprite.tint.b, sprite.tint.a];
    for v in tint {
        frame_bytes.extend_from_slice(&v.to_le_bytes());
    }

    let uniform =
        device.create_buffer_init_via_queue(&queue, &frame_bytes, wgpu::BufferUsages::UNIFORM);

    // Atlas texture upload.
    let atlas = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("atlas"),
        size: wgpu::Extent3d {
            width: atlas_width,
            height: atlas_height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &atlas,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        atlas_rgba8,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas_width * 4),
            rows_per_image: Some(atlas_height),
        },
        wgpu::Extent3d {
            width: atlas_width,
            height: atlas_height,
            depth_or_array_layers: 1,
        },
    );
    let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("nearest"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    let group0_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("g0"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let group1_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("g1"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sprite3d"),
        bind_group_layouts: &[&group0_layout, &group1_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sprite3d"),
        layout: Some(&pipeline_layout),
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
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    let g0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("g0"),
        layout: &group0_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    let g1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("g1"),
        layout: &group1_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());

    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("draw"),
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
                        b: 0.05,
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
        pass.set_bind_group(0, &g0, &[]);
        pass.set_bind_group(1, &g1, &[]);
        pass.draw(0..6, 0..1);
    }

    // Texture → readback buffer.
    let unpadded_row = width * 4;
    let padded_row = align_up(unpadded_row, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buf_size = u64::from(padded_row) * u64::from(height);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: buf_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(enc.finish()));

    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::Wait)
        .map_err(|e| SpriteRenderError::PixelLayout(format!("{e:?}")))?;
    let data = slice.get_mapped_range();
    // Strip padding row-by-row.
    let mut out = Vec::with_capacity((unpadded_row * height) as usize);
    for row in 0..height {
        let start = (row * padded_row) as usize;
        let end = start + unpadded_row as usize;
        out.extend_from_slice(&data[start..end]);
    }
    drop(data);
    readback.unmap();
    Ok(out)
}

#[inline]
const fn align_up(v: u32, alignment: u32) -> u32 {
    (v + alignment - 1) & !(alignment - 1)
}

/// Build a simple orthographic projection matrix in column-major order
/// (matches wgpu / glam expectations).
fn orthographic_matrix(l: f32, r: f32, b: f32, t: f32, n: f32, f: f32) -> [f32; 16] {
    let rl = r - l;
    let tb = t - b;
    let fne = f - n;
    [
        2.0 / rl,
        0.0,
        0.0,
        0.0,
        0.0,
        2.0 / tb,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0 / fne,
        0.0,
        -(r + l) / rl,
        -(t + b) / tb,
        -n / fne,
        1.0,
    ]
}

/// Tiny helper for one-shot uniform buffer creation; declared here so the
/// public surface stays minimal (no extra dependency on `wgpu::util`).
trait BufferInitExt {
    fn create_buffer_init_via_queue(
        &self,
        queue: &wgpu::Queue,
        bytes: &[u8],
        extra_usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer;
}

impl BufferInitExt for wgpu::Device {
    fn create_buffer_init_via_queue(
        &self,
        queue: &wgpu::Queue,
        bytes: &[u8],
        extra_usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        // 4-byte align (uniform spec).
        let len = ((bytes.len() + 3) & !3) as u64;
        let buf = self.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform"),
            size: len,
            usage: extra_usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buf, 0, bytes);
        buf
    }
}

/// WGSL for an SDF-glyph quad — samples a luminance-encoded SDF where
/// values around `128` mark the glyph contour, applies a `smoothstep`
/// edge, and tints the result.
#[must_use]
pub const fn sdf_glyph_wgsl() -> &'static str {
    r"
struct Frame {
    view_proj: mat4x4<f32>,
    sprite_pos: vec4<f32>,
    tint:       vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>(-0.5,  0.5),
        vec2<f32>(-0.5,  0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>( 0.5,  0.5),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );
    let p = positions[vid] * frame.sprite_pos.w;
    let world = vec4<f32>(p.x + frame.sprite_pos.x,
                          p.y + frame.sprite_pos.y,
                          frame.sprite_pos.z,
                          1.0);
    var out: VsOut;
    out.clip_pos = frame.view_proj * world;
    out.uv = uvs[vid];
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Atlas stores SDF where >0.5 = inside the glyph (we map 0..255 → 0..1).
    let s = textureSample(atlas, atlas_sampler, in.uv).r;
    let edge = 0.5;
    let smoothing = 0.06;
    let alpha = smoothstep(edge - smoothing, edge + smoothing, s);
    var color = frame.tint;
    color.a = color.a * alpha;
    return color;
}
"
}

/// Render a single SDF glyph (e.g. from [`crate::bridge::SdfFontProvider`])
/// to an RGBA8 buffer. `glyph_sdf` is a `glyph_w × glyph_h` array of
/// 0..255 values where pixels around `128` are at the glyph contour.
///
/// # Errors
/// Returns [`SpriteRenderError`] when the wgpu adapter / device fail to
/// initialise.
#[cfg(feature = "gpu")]
pub fn render_glyph_to_rgba8(
    glyph_sdf: &[u8],
    glyph_w: u32,
    glyph_h: u32,
    tint: crate::math::Color,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, SpriteRenderError> {
    // Repack the single-channel SDF as RGBA8 (stick the value into all
    // four channels so a generic texture format works).
    assert_eq!(glyph_sdf.len(), (glyph_w * glyph_h) as usize);
    let mut atlas = Vec::with_capacity((glyph_w * glyph_h * 4) as usize);
    for &s in glyph_sdf {
        atlas.extend_from_slice(&[s, s, s, 255]);
    }
    let sprite = Sprite3D {
        position: crate::math::Vec3::ZERO,
        size: 1.5,
        texture_id: 0,
        tint,
        mode: crate::hd2d_postfx::SpriteMode::Unlit,
        billboard_bias: 1.0,
    };
    render_with_wgsl(
        sdf_glyph_wgsl(),
        &sprite,
        &atlas,
        glyph_w,
        glyph_h,
        width,
        height,
    )
}

/// Internal helper that the two `render_*` entry points share. Lets us
/// pick a different WGSL fragment stage while reusing the rest of the
/// pipeline scaffolding.
#[cfg(feature = "gpu")]
fn render_with_wgsl(
    wgsl: &str,
    sprite: &Sprite3D,
    atlas_rgba8: &[u8],
    atlas_width: u32,
    atlas_height: u32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, SpriteRenderError> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .map_err(|_| SpriteRenderError::NoAdapter)?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("glyph_render"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
    }))
    .map_err(|e| SpriteRenderError::NoDevice(format!("{e:?}")))?;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("frag"),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    let view_proj = orthographic_matrix(-1.5, 1.5, -1.5, 1.5, -1.0, 1.0);
    let mut frame_bytes = Vec::with_capacity(96);
    for v in view_proj {
        frame_bytes.extend_from_slice(&v.to_le_bytes());
    }
    for v in [
        sprite.position.x(),
        sprite.position.y(),
        sprite.position.z(),
        sprite.size,
    ] {
        frame_bytes.extend_from_slice(&v.to_le_bytes());
    }
    let tint = [sprite.tint.r, sprite.tint.g, sprite.tint.b, sprite.tint.a];
    for v in tint {
        frame_bytes.extend_from_slice(&v.to_le_bytes());
    }
    let uniform =
        device.create_buffer_init_via_queue(&queue, &frame_bytes, wgpu::BufferUsages::UNIFORM);

    let atlas_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("atlas"),
        size: wgpu::Extent3d {
            width: atlas_width,
            height: atlas_height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &atlas_tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        atlas_rgba8,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas_width * 4),
            rows_per_image: Some(atlas_height),
        },
        wgpu::Extent3d {
            width: atlas_width,
            height: atlas_height,
            depth_or_array_layers: 1,
        },
    );
    let atlas_view = atlas_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("nearest"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let group0_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("g0"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let group1_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("g1"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("glyph"),
        bind_group_layouts: &[&group0_layout, &group1_layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("glyph"),
        layout: Some(&pipeline_layout),
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
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
    let g0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("g0"),
        layout: &group0_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    let g1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("g1"),
        layout: &group1_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("draw"),
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
        pass.set_bind_group(0, &g0, &[]);
        pass.set_bind_group(1, &g1, &[]);
        pass.draw(0..6, 0..1);
    }
    let unpadded_row = width * 4;
    let padded_row = align_up(unpadded_row, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buf_size = u64::from(padded_row) * u64::from(height);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: buf_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(enc.finish()));

    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::Wait)
        .map_err(|e| SpriteRenderError::PixelLayout(format!("{e:?}")))?;
    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((unpadded_row * height) as usize);
    for row in 0..height {
        let start = (row * padded_row) as usize;
        let end = start + unpadded_row as usize;
        out.extend_from_slice(&data[start..end]);
    }
    drop(data);
    readback.unmap();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hd2d_postfx::Sprite3D;
    use crate::math::{Color, Vec3};

    /// 4×4 solid-red atlas.
    fn red_atlas() -> (Vec<u8>, u32, u32) {
        let mut bytes = Vec::with_capacity(16 * 4);
        for _ in 0..16 {
            bytes.extend_from_slice(&[255, 0, 0, 255]);
        }
        (bytes, 4, 4)
    }

    #[test]
    fn pipeline_wgsl_parses_with_naga() {
        let m = naga::front::wgsl::parse_str(sprite3d_pipeline_wgsl()).expect("parse");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&m)
        .expect("validate");
    }

    #[cfg(feature = "gpu")]
    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn sprite_render_produces_red_pixel_in_center() {
        let (atlas, aw, ah) = red_atlas();
        let mut sprite = Sprite3D::new(Vec3::ZERO, 1.5, 0);
        sprite.tint = Color::WHITE;
        let rgba = render_sprite_to_rgba8(&sprite, &atlas, aw, ah, 64, 64).expect("render");
        assert_eq!(rgba.len(), 64 * 64 * 4);
        let off = (32 * 64 + 32) * 4;
        let r = rgba[off];
        let g = rgba[off + 1];
        let b = rgba[off + 2];
        // The sprite tint was white over a solid-red atlas, so the central
        // pixel should be dominantly red.
        assert!(
            r > 200 && g < 60 && b < 60,
            "expected red, got ({r},{g},{b})"
        );
    }

    #[test]
    fn sdf_glyph_wgsl_parses_with_naga() {
        let m = naga::front::wgsl::parse_str(sdf_glyph_wgsl()).expect("parse");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&m)
        .expect("validate");
    }

    #[cfg(feature = "gpu")]
    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn glyph_render_inside_is_tinted_outside_transparent() {
        // 8×8 SDF where the inside half (top rows) is filled (255) and the
        // outside half (bottom rows) is empty (0). After rendering, the
        // central pixel should be the tint colour with full alpha; a
        // pixel near the bottom edge should be the clear-colour black.
        let mut sdf = Vec::with_capacity(8 * 8);
        for y in 0..8 {
            for _x in 0..8 {
                sdf.push(if y < 4 { 255 } else { 0 });
            }
        }
        let tint = Color::new(1.0, 0.5, 0.2, 1.0);
        let rgba = render_glyph_to_rgba8(&sdf, 8, 8, tint, 64, 64).expect("render");
        // Sample 18,32 — somewhere clearly inside the top half.
        let off = (18 * 64 + 32) * 4;
        let r = rgba[off];
        let g = rgba[off + 1];
        let b = rgba[off + 2];
        assert!(
            r > 200 && g > 80 && g < 180 && b < 80,
            "expected tinted inside pixel, got ({r},{g},{b})"
        );
        // Sample near the bottom — should be near-zero (cleared).
        let off2 = (50 * 64 + 32) * 4;
        let rb = rgba[off2];
        let gb = rgba[off2 + 1];
        let bb = rgba[off2 + 2];
        assert!(
            rb < 30 && gb < 30 && bb < 30,
            "expected cleared outside pixel, got ({rb},{gb},{bb})"
        );
    }

    #[cfg(feature = "gpu")]
    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn sprite_render_respects_tint() {
        let (atlas, aw, ah) = red_atlas();
        let mut sprite = Sprite3D::new(Vec3::ZERO, 1.5, 0);
        sprite.tint = Color::new(0.0, 0.0, 1.0, 1.0); // multiply red by blue → black
        let rgba = render_sprite_to_rgba8(&sprite, &atlas, aw, ah, 64, 64).expect("render");
        let off = (32 * 64 + 32) * 4;
        let r = rgba[off];
        let g = rgba[off + 1];
        let b = rgba[off + 2];
        // tint blue × atlas red → black (0,0,0).
        assert!(
            r < 30 && g < 30 && b < 30,
            "expected near-black, got ({r},{g},{b})"
        );
    }
}
