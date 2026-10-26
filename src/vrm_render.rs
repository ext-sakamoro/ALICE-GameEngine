//! Drawing a posed VRM primitive into an offscreen image with a two-tone (toon) shader.
//!
//! The vertices come already posed ([`crate::vrm_pose::VrmPose::posed_positions`]), so the
//! skinning and the morph targets are the CPU's, one implementation for both the image and
//! anything that hashes or compares the pose. The shader is `MToon` 1.0's shading:
//! `s = dot(normal, light) + shift`, `t = clamp((s + 1 - toony) / (2 - 2 toony), 0, 1)` (a step at
//! `s = 0` when toony is 1), colour = `mix(shade, base, t)`. [`toon_wgsl`] is plain text (validated by naga in the tests); the renderer is
//! behind the `gpu` feature.

/// The WGSL of the toon pipeline: `vs_main` / `fs_main`, one uniform block at group 0
#[must_use]
pub const fn toon_wgsl() -> &'static str {
    r"
struct Params {
    view_proj: mat4x4<f32>,
    base_color: vec4<f32>,
    shade_color: vec4<f32>,
    // xyz: the direction towards the light; w: the shading shift
    light: vec4<f32>,
    // x: the shading toony factor (0..=1)
    shading: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Params;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = params.view_proj * vec4<f32>(v.position, 1.0);
    out.normal = v.normal;
    return out;
}

@fragment
fn fs_main(f: VsOut) -> @location(0) vec4<f32> {
    let s = dot(normalize(f.normal), normalize(params.light.xyz)) + params.light.w;
    let toony = params.shading.x;
    let width = 2.0 - 2.0 * toony;
    var t: f32;
    if (width <= 0.0) {
        t = select(0.0, 1.0, s >= 0.0);
    } else {
        t = clamp((s + 1.0 - toony) / width, 0.0, 1.0);
    }
    return mix(params.shade_color, params.base_color, t);
}
"
}

/// What a draw is lit and seen with
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToonParams {
    /// Column-major view-projection matrix
    pub view_proj: [f32; 16],
    /// The colour of a lit fragment
    pub base_color: [f32; 4],
    /// The colour of a fragment in shade
    pub shade_color: [f32; 4],
    /// The direction towards the light (need not be unit length)
    pub light_dir: [f32; 3],
    /// `MToon` `shadingShiftFactor`: added to `dot(normal, light)`
    pub shading_shift: f32,
    /// `MToon` `shadingToonyFactor` (0..=1): 1 is a hard step at 0, lower values a wider ramp
    pub shading_toony: f32,
    /// The colour the image is cleared to
    pub clear: [f32; 4],
}

impl ToonParams {
    /// The parameters of a material: its base colour, and its `MToon` shade colour, shift and
    /// toony (a material without `MToon` is drawn in its base colour alone: shade = base)
    #[must_use]
    pub fn from_material(
        m: &crate::vrm::Material,
        view_proj: [f32; 16],
        light_dir: [f32; 3],
        clear: [f32; 4],
    ) -> Self {
        let (shade, shift, toony) = m.mtoon.map_or(
            (
                [m.base_color[0], m.base_color[1], m.base_color[2]],
                0.0,
                1.0,
            ),
            |t| (t.shade_color, t.shading_shift, t.shading_toony),
        );
        Self {
            view_proj,
            base_color: m.base_color,
            shade_color: [shade[0], shade[1], shade[2], m.base_color[3]],
            light_dir,
            shading_shift: shift,
            shading_toony: toony,
            clear,
        }
    }
}

/// Why a draw failed
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderError {
    /// No GPU adapter or device
    NoDevice(String),
    /// The vertex data does not describe triangles of the given vertices
    BadMesh(String),
    /// The image could not be read back
    Readback(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDevice(e) => write!(f, "no GPU device: {e}"),
            Self::BadMesh(e) => write!(f, "bad mesh: {e}"),
            Self::Readback(e) => write!(f, "readback failed: {e}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// Checks a mesh before anything is uploaded: as many normals as positions, every index names a
/// vertex, the index count is a multiple of 3, and every number is finite
///
/// # Errors
///
/// [`RenderError::BadMesh`]
pub fn check_mesh(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    indices: &[u32],
) -> Result<(), RenderError> {
    if positions.len() != normals.len() {
        return Err(RenderError::BadMesh(format!(
            "{} positions and {} normals",
            positions.len(),
            normals.len()
        )));
    }
    if indices.is_empty() || !indices.len().is_multiple_of(3) {
        return Err(RenderError::BadMesh(format!(
            "{} indices is not a whole number of triangles",
            indices.len()
        )));
    }
    if let Some(i) = indices.iter().find(|&&i| i as usize >= positions.len()) {
        return Err(RenderError::BadMesh(format!(
            "index {i} past {} vertices",
            positions.len()
        )));
    }
    if !positions
        .iter()
        .chain(normals)
        .flatten()
        .all(|x| x.is_finite())
    {
        return Err(RenderError::BadMesh("a coordinate is not finite".into()));
    }
    Ok(())
}

#[cfg(feature = "gpu")]
pub use gpu::ToonRenderer;

#[cfg(feature = "gpu")]
mod gpu {
    use super::{check_mesh, toon_wgsl, RenderError, ToonParams};
    use wgpu::util::DeviceExt;

    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
    const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    /// An offscreen toon renderer of one size
    pub struct ToonRenderer {
        device: wgpu::Device,
        queue: wgpu::Queue,
        pipeline: wgpu::RenderPipeline,
        layout: wgpu::BindGroupLayout,
        width: u32,
        height: u32,
    }

    impl ToonRenderer {
        /// A renderer on the default adapter, drawing `width` x `height` images
        ///
        /// # Errors
        ///
        /// [`RenderError::NoDevice`]
        pub fn new_headless(width: u32, height: u32) -> Result<Self, RenderError> {
            if width == 0 || height == 0 || width > 8192 || height > 8192 {
                return Err(RenderError::BadMesh(format!(
                    "image size {width} x {height}"
                )));
            }
            let instance = wgpu::Instance::default();
            let adapter = pollster::block_on(
                instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
            )
            .map_err(|e| RenderError::NoDevice(e.to_string()))?;
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    label: Some("vrm toon"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults(),
                    memory_hints: wgpu::MemoryHints::default(),
                    trace: wgpu::Trace::Off,
                }))
                .map_err(|e| RenderError::NoDevice(e.to_string()))?;
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("vrm toon"),
                source: wgpu::ShaderSource::Wgsl(toon_wgsl().into()),
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("vrm toon params"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vrm toon"),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
            let vertex_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("vrm toon"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 24,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vertex_attrs,
                    }],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });
            Ok(Self {
                device,
                queue,
                pipeline,
                layout,
                width,
                height,
            })
        }

        /// Draws one primitive (both faces) and returns the image as RGBA8 rows, top row first
        ///
        /// # Errors
        ///
        /// [`RenderError::BadMesh`] or [`RenderError::Readback`]
        #[allow(clippy::too_many_lines)]
        pub fn draw(
            &self,
            positions: &[[f32; 3]],
            normals: &[[f32; 3]],
            indices: &[u32],
            p: &ToonParams,
        ) -> Result<Vec<u8>, RenderError> {
            check_mesh(positions, normals, indices)?;
            let verts: Vec<f32> = positions
                .iter()
                .zip(normals)
                .flat_map(|(a, n)| a.iter().chain(n).copied())
                .collect();
            let vbuf = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("vrm toon vertices"),
                    contents: bytemuck::cast_slice(&verts),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let ibuf = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("vrm toon indices"),
                    contents: bytemuck::cast_slice(indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
            let mut uni: Vec<f32> = p.view_proj.to_vec();
            uni.extend_from_slice(&p.base_color);
            uni.extend_from_slice(&p.shade_color);
            uni.extend_from_slice(&[
                p.light_dir[0],
                p.light_dir[1],
                p.light_dir[2],
                p.shading_shift,
            ]);
            uni.extend_from_slice(&[p.shading_toony, 0.0, 0.0, 0.0]);
            let ubuf = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("vrm toon params"),
                    contents: bytemuck::cast_slice(&uni),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vrm toon params"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ubuf.as_entire_binding(),
                }],
            });
            let size = wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            };
            let target = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vrm toon target"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let depth = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vrm toon depth"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let tview = target.create_view(&wgpu::TextureViewDescriptor::default());
            let dview = depth.create_view(&wgpu::TextureViewDescriptor::default());
            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("vrm toon"),
                });
            {
                let [r, g, b, a] = p.clear.map(f64::from);
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("vrm toon"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &tview,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &dview,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                #[allow(clippy::cast_possible_truncation)]
                pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
            }
            let row = self.width * 4;
            let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
                * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
            let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vrm toon readback"),
                size: u64::from(padded) * u64::from(self.height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
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
                        bytes_per_row: Some(padded),
                        rows_per_image: Some(self.height),
                    },
                },
                size,
            );
            self.queue.submit(Some(enc.finish()));
            let slice = readback.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
            self.device
                .poll(wgpu::PollType::Wait)
                .map_err(|e| RenderError::Readback(e.to_string()))?;
            rx.recv()
                .map_err(|e| RenderError::Readback(e.to_string()))?
                .map_err(|e| RenderError::Readback(e.to_string()))?;
            let data = slice.get_mapped_range();
            let mut out = Vec::with_capacity((row * self.height) as usize);
            for y in 0..self.height as usize {
                let s = y * padded as usize;
                out.extend_from_slice(&data[s..s + row as usize]);
            }
            drop(data);
            readback.unmap();
            Ok(out)
        }
    }
}
