//! Drawing a posed VRM primitive into an offscreen image with a two-tone (toon) shader.
//!
//! The vertices come already posed ([`crate::vrm_pose::VrmPose::posed_positions`]), so the
//! skinning and the morph targets are the CPU's, one implementation for both the image and
//! anything that hashes or compares the pose. The shader is `MToon` 1.0's shading:
//! `s = dot(normal, light) + shift`, `t = clamp((s + 1 - toony) / (2 - 2 toony), 0, 1)` (a step at
//! `s = 0` when toony is 1), colour = `mix(shade, base, t)`. A part may carry a [`FacePaint`]:
//! 2D shapes (ellipses and strokes) on a plane in front of the face, evaluated per fragment as
//! signed distances, so eyes, brows and a mouth are laws on the surface rather than meshes, and
//! an expression is a change of the shapes' numbers. [`toon_wgsl`] is plain text (validated by
//! naga in the tests); the renderer is behind the `gpu` feature.

/// The WGSL of the toon pipeline: `vs_main` / `fs_main` for the surface and `vs_outline` /
/// `fs_outline` for its inverted-hull outline, one uniform block at group 0
#[must_use]
#[allow(clippy::too_many_lines)] // the shader's text
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
    // x: outline width, y: mode (0 none, 1 world, 2 screen), z: lighting mix, w: image aspect
    outline: vec4<f32>,
    // rgb: outline colour
    outline_color: vec4<f32>,
    // world -> face plane: xy are the paint coordinates, z the depth along the face's forward
    to_face: mat4x4<f32>,
    // x: the number of shapes, y: the least face depth that is painted
    paint: vec4<f32>,
    // 3 vec4 per shape: (kind, p0.x, p0.y, p1.x), (p1.y, p2.x, p2.y, half width), colour
    shapes: array<vec4<f32>, 96>,
};

@group(0) @binding(0) var<uniform> params: Params;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world: vec3<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = params.view_proj * vec4<f32>(v.position, 1.0);
    out.normal = v.normal;
    out.world = v.position;
    return out;
}

// The outline hull: the surface pushed out along its normal, drawn with front faces culled
// (MToon 1.0): in world mode by the width in model units, in screen mode by the width as a
// fraction of the image height, in the direction the normal points on screen
@vertex
fn vs_outline(v: VsIn) -> VsOut {
    var out: VsOut;
    let n = normalize(v.normal);
    let width = params.outline.x;
    if (params.outline.y < 1.5) {
        out.clip = params.view_proj * vec4<f32>(v.position + n * width, 1.0);
    } else {
        let c = params.view_proj * vec4<f32>(v.position, 1.0);
        let aspect = params.outline.w;
        // the normal's direction in clip space (as UniVRM's MToon takes it)
        let nc = (params.view_proj * vec4<f32>(n, 0.0)).xy;
        let len = length(nc);
        var dir = vec2<f32>(0.0, 0.0);
        if (len > 0.0) {
            dir = nc / len;
        }
        // NDC spans 2 over the image height; x is divided by the aspect so a width across is the
        // same number of pixels as one up; times w, so the width holds at any depth
        let off = dir * (2.0 * width) * vec2<f32>(1.0 / aspect, 1.0);
        out.clip = vec4<f32>(c.xy + off * c.w, c.zw);
    }
    out.normal = v.normal;
    out.world = v.position;
    return out;
}

fn segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;
    let h = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-12), 0.0, 1.0);
    return length(p - a - ab * h);
}

fn bez(a: vec2<f32>, b: vec2<f32>, c: vec2<f32>, t: f32) -> vec2<f32> {
    let u = 1.0 - t;
    return u * u * a + 2.0 * u * t * b + t * t * c;
}

// signed distance of shape i at p (negative inside)
fn shape_distance(i: u32, p: vec2<f32>) -> f32 {
    let s0 = params.shapes[3u * i];
    let s1 = params.shapes[3u * i + 1u];
    if (s0.x < 0.5) {
        // ellipse: centre s0.yz, radii (s0.w, s1.x); the sign is exact
        let r = vec2<f32>(s0.w, s1.x);
        return (length((p - s0.yz) / r) - 1.0) * min(r.x, r.y);
    }
    // stroke: the quadratic curve a = s0.yz, b = (s0.w, s1.x), c = s1.yz, sampled at 9 points
    let a = s0.yz;
    let b = vec2<f32>(s0.w, s1.x);
    let c = s1.yz;
    var d = 1e9;
    var prev = a;
    for (var k = 1u; k <= 8u; k = k + 1u) {
        let next = bez(a, b, c, f32(k) / 8.0);
        d = min(d, segment(p, prev, next));
        prev = next;
    }
    return d - s1.w;
}

// the surface's toon colour at a normal (MToon 1.0 shading)
fn toon(normal: vec3<f32>) -> vec4<f32> {
    let s = dot(normalize(normal), normalize(params.light.xyz)) + params.light.w;
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

// the outline colour, tinted by the surface's lit colour by the lighting mix
@fragment
fn fs_outline(f: VsOut) -> @location(0) vec4<f32> {
    let lit = toon(f.normal).rgb;
    let c = params.outline_color.rgb * mix(vec3<f32>(1.0, 1.0, 1.0), lit, params.outline.z);
    return vec4<f32>(c, 1.0);
}

@fragment
fn fs_main(f: VsOut) -> @location(0) vec4<f32> {
    var color = toon(f.normal);
    let count = u32(params.paint.x);
    if (count > 0u) {
        let q = (params.to_face * vec4<f32>(f.world, 1.0)).xyz;
        if (q.z >= params.paint.y) {
            // in order: a later shape is drawn over an earlier one, flat (unlit)
            for (var i = 0u; i < count; i = i + 1u) {
                if (shape_distance(i, q.xy) <= 0.0) {
                    color = params.shapes[3u * i + 2u];
                }
            }
        }
    }
    return color;
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

/// The most shapes a [`FacePaint`] holds
pub const MAX_PAINT_SHAPES: usize = 32;

/// A 2D shape of a [`FacePaint`], in face-plane coordinates
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaintShape {
    /// The inside of an axis-aligned ellipse
    Ellipse {
        /// Centre
        center: [f32; 2],
        /// Radii along x and y (both > 0)
        radii: [f32; 2],
        /// Colour
        color: [f32; 4],
    },
    /// Every point within `half_width` of the quadratic curve from `a` through control `b` to
    /// `c` (the curve is sampled at 9 points, t = 0, 1/8, ..., 1)
    Stroke {
        /// Start
        a: [f32; 2],
        /// Control point
        b: [f32; 2],
        /// End
        c: [f32; 2],
        /// Half the width (> 0)
        half_width: f32,
        /// Colour
        color: [f32; 4],
    },
}

/// Shapes painted on a part: a fragment at world `w` is taken to the face plane by `to_face`
/// (`q = to_face * (w, 1)`), and where `q.z >= min_depth` the last shape whose signed distance
/// at `q.xy` is <= 0 gives its colour, flat (unlit)
#[derive(Debug, Clone, PartialEq)]
pub struct FacePaint {
    /// Column-major world -> face-plane matrix
    pub to_face: [f32; 16],
    /// The least face depth that is painted (keeps the back of a head clear)
    pub min_depth: f32,
    /// The shapes, drawn in order (at most [`MAX_PAINT_SHAPES`])
    pub shapes: Vec<PaintShape>,
}

/// Checks a paint: at most [`MAX_PAINT_SHAPES`] shapes, every number finite, radii and half
/// widths positive
///
/// # Errors
///
/// [`RenderError::BadPaint`]
pub fn check_paint(p: &FacePaint) -> Result<(), RenderError> {
    if p.shapes.len() > MAX_PAINT_SHAPES {
        return Err(RenderError::BadPaint(format!(
            "{} shapes, at most {MAX_PAINT_SHAPES}",
            p.shapes.len()
        )));
    }
    if !p.to_face.iter().all(|x| x.is_finite()) || !p.min_depth.is_finite() {
        return Err(RenderError::BadPaint("a non-finite matrix or depth".into()));
    }
    for (i, s) in p.shapes.iter().enumerate() {
        let (nums, size): (Vec<f32>, [f32; 2]) = match *s {
            PaintShape::Ellipse {
                center,
                radii,
                color,
            } => (
                center.iter().chain(&radii).chain(&color).copied().collect(),
                radii,
            ),
            PaintShape::Stroke {
                a,
                b,
                c,
                half_width,
                color,
            } => (
                a.iter()
                    .chain(&b)
                    .chain(&c)
                    .chain(&[half_width])
                    .chain(&color)
                    .copied()
                    .collect(),
                [half_width; 2],
            ),
        };
        if !nums.iter().all(|x| x.is_finite()) {
            return Err(RenderError::BadPaint(format!(
                "shape {i}: a non-finite number"
            )));
        }
        if size.iter().any(|&r| r <= 0.0) {
            return Err(RenderError::BadPaint(format!(
                "shape {i}: a radius or half width that is not positive"
            )));
        }
    }
    Ok(())
}

/// The uniform words of a paint (after the toon parameters): the matrix, the header and 96 vec4
fn paint_words(p: Option<&FacePaint>) -> Vec<f32> {
    let mut w = Vec::with_capacity(16 + 4 + 96 * 4);
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    w.extend_from_slice(&p.map_or(identity, |p| p.to_face));
    #[allow(clippy::cast_precision_loss)]
    let count = p.map_or(0, |p| p.shapes.len()) as f32;
    w.extend_from_slice(&[count, p.map_or(0.0, |p| p.min_depth), 0.0, 0.0]);
    for s in p.map_or(&[][..], |p| &p.shapes[..]) {
        match *s {
            PaintShape::Ellipse {
                center,
                radii,
                color,
            } => {
                w.extend_from_slice(&[0.0, center[0], center[1], radii[0]]);
                w.extend_from_slice(&[radii[1], 0.0, 0.0, 0.0]);
                w.extend_from_slice(&color);
            }
            PaintShape::Stroke {
                a,
                b,
                c,
                half_width,
                color,
            } => {
                w.extend_from_slice(&[1.0, a[0], a[1], b[0]]);
                w.extend_from_slice(&[b[1], c[0], c[1], half_width]);
                w.extend_from_slice(&color);
            }
        }
    }
    w.resize(16 + 4 + 96 * 4, 0.0);
    w
}

/// An inverted-hull outline (`MToon` 1.0's outline)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outline {
    /// `World` (width in model units) or `Screen` (width as a fraction of the image height)
    pub mode: OutlineMode,
    /// The width (not negative)
    pub width: f32,
    /// The colour
    pub color: [f32; 3],
    /// How much the surface's lit colour tints the outline (0..=1): colour * mix(1, lit, this)
    pub lighting_mix: f32,
}

/// How an [`Outline`]'s width is measured
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineMode {
    /// In model units, so the line thins with distance
    World,
    /// As a fraction of the image height, the same at any distance
    Screen,
}

impl Outline {
    /// The outline of a material: `None` when it has no `MToon`, its mode is `none` or its width
    /// is 0
    #[must_use]
    pub fn from_material(m: &crate::vrm::Material) -> Option<Self> {
        let t = m.mtoon?;
        let mode = match t.outline_width_mode {
            crate::vrm::OutlineWidthMode::None => return None,
            crate::vrm::OutlineWidthMode::World => OutlineMode::World,
            crate::vrm::OutlineWidthMode::Screen => OutlineMode::Screen,
        };
        (t.outline_width > 0.0).then_some(Self {
            mode,
            width: t.outline_width,
            color: t.outline_color,
            lighting_mix: t.outline_lighting_mix,
        })
    }
}

fn check_outline(o: &Outline) -> Result<(), RenderError> {
    if !(o.width.is_finite() && o.width >= 0.0) {
        return Err(RenderError::BadOutline(format!("width {}", o.width)));
    }
    if !(0.0..=1.0).contains(&o.lighting_mix) {
        return Err(RenderError::BadOutline(format!(
            "lighting mix {}",
            o.lighting_mix
        )));
    }
    if !o.color.iter().all(|x| x.is_finite()) {
        return Err(RenderError::BadOutline("a non-finite colour".into()));
    }
    Ok(())
}

/// One primitive of a multi-part draw: its mesh and the parameters it is lit and seen with
#[derive(Debug, Clone, Copy)]
pub struct ToonPart<'a> {
    /// Vertex positions
    pub positions: &'a [[f32; 3]],
    /// Vertex normals, one per position
    pub normals: &'a [[f32; 3]],
    /// Triangle indices
    pub indices: &'a [u32],
    /// The material, light and view of this part (its `clear` is not used)
    pub params: ToonParams,
    /// Shapes painted on this part, if any
    pub paint: Option<&'a FacePaint>,
    /// The part's outline, if any
    pub outline: Option<Outline>,
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
    /// A face paint that [`check_paint`] refuses
    BadPaint(String),
    /// An outline with a negative or non-finite width, a lighting mix outside 0..=1 or a
    /// non-finite colour
    BadOutline(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDevice(e) => write!(f, "no GPU device: {e}"),
            Self::BadMesh(e) => write!(f, "bad mesh: {e}"),
            Self::Readback(e) => write!(f, "readback failed: {e}"),
            Self::BadPaint(e) => write!(f, "bad paint: {e}"),
            Self::BadOutline(e) => write!(f, "bad outline: {e}"),
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
    use super::{
        check_mesh, check_outline, check_paint, paint_words, toon_wgsl, OutlineMode, RenderError,
        ToonParams, ToonPart,
    };
    use wgpu::util::DeviceExt;

    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
    const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    /// An offscreen toon renderer of one size
    pub struct ToonRenderer {
        device: wgpu::Device,
        queue: wgpu::Queue,
        pipeline: wgpu::RenderPipeline,
        outline_pipeline: wgpu::RenderPipeline,
        layout: wgpu::BindGroupLayout,
        width: u32,
        height: u32,
    }

    /// A pipeline of the toon shader with these entry points, culling `cull`
    fn toon_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        vs: &str,
        fs: &str,
        cull: Option<wgpu::Face>,
    ) -> wgpu::RenderPipeline {
        let vertex_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(vs),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(vs),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &vertex_attrs,
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fs),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: cull,
                ..wgpu::PrimitiveState::default()
            },
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
        })
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
            let pipeline = toon_pipeline(&device, &shader, &pl, "vs_main", "fs_main", None);
            // the hull's front faces would cover the surface; its back faces show around it
            let outline_pipeline = toon_pipeline(
                &device,
                &shader,
                &pl,
                "vs_outline",
                "fs_outline",
                Some(wgpu::Face::Front),
            );
            Ok(Self {
                device,
                queue,
                pipeline,
                outline_pipeline,
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
        pub fn draw(
            &self,
            positions: &[[f32; 3]],
            normals: &[[f32; 3]],
            indices: &[u32],
            p: &ToonParams,
        ) -> Result<Vec<u8>, RenderError> {
            self.draw_parts(
                &[ToonPart {
                    positions,
                    normals,
                    indices,
                    params: *p,
                    paint: None,
                    outline: None,
                }],
                p.clear,
            )
        }

        /// Draws the parts (both faces of each) into one image with one depth buffer, so a nearer
        /// fragment hides a farther one whatever the order of the parts, and returns the image as
        /// RGBA8 rows, top row first. Each part has its own material, light and view
        ///
        /// # Errors
        ///
        /// [`RenderError::BadMesh`] (no part, or a part whose mesh `check_mesh` refuses, named by
        /// its index) or [`RenderError::Readback`]
        #[allow(clippy::too_many_lines)]
        pub fn draw_parts(
            &self,
            parts: &[ToonPart<'_>],
            clear: [f32; 4],
        ) -> Result<Vec<u8>, RenderError> {
            if parts.is_empty() {
                return Err(RenderError::BadMesh("no part to draw".into()));
            }
            for (i, part) in parts.iter().enumerate() {
                check_mesh(part.positions, part.normals, part.indices)
                    .map_err(|e| RenderError::BadMesh(format!("part {i}: {e}")))?;
                if let Some(o) = &part.outline {
                    check_outline(o).map_err(|e| match e {
                        RenderError::BadOutline(m) => {
                            RenderError::BadOutline(format!("part {i}: {m}"))
                        }
                        other => other,
                    })?;
                }
                if let Some(paint) = part.paint {
                    check_paint(paint).map_err(|e| match e {
                        RenderError::BadPaint(m) => RenderError::BadPaint(format!("part {i}: {m}")),
                        other => other,
                    })?;
                }
            }
            let uploads: Vec<_> = parts
                .iter()
                .map(|part| {
                    let p = &part.params;
                    let verts: Vec<f32> = part
                        .positions
                        .iter()
                        .zip(part.normals)
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
                            contents: bytemuck::cast_slice(part.indices),
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
                    #[allow(clippy::cast_precision_loss)]
                    let aspect = self.width as f32 / self.height as f32;
                    let (width, mode, mix, color) =
                        part.outline.map_or((0.0, 0.0, 0.0, [0.0; 3]), |o| {
                            let mode = match o.mode {
                                OutlineMode::World => 1.0,
                                OutlineMode::Screen => 2.0,
                            };
                            (o.width, mode, o.lighting_mix, o.color)
                        });
                    uni.extend_from_slice(&[width, mode, mix, aspect]);
                    uni.extend_from_slice(&[color[0], color[1], color[2], 1.0]);
                    uni.extend(paint_words(part.paint));
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
                    #[allow(clippy::cast_possible_truncation)]
                    let count = part.indices.len() as u32;
                    let outlined = part.outline.is_some_and(|o| o.width > 0.0);
                    (vbuf, ibuf, bind, count, outlined)
                })
                .collect();
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
                let [r, g, b, a] = clear.map(f64::from);
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
                // the outline hulls, then the surfaces (the depth buffer orders them)
                pass.set_pipeline(&self.outline_pipeline);
                for (vbuf, ibuf, bind, count, outlined) in &uploads {
                    if *outlined {
                        pass.set_bind_group(0, bind, &[]);
                        pass.set_vertex_buffer(0, vbuf.slice(..));
                        pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..*count, 0, 0..1);
                    }
                }
                pass.set_pipeline(&self.pipeline);
                for (vbuf, ibuf, bind, count, _) in &uploads {
                    pass.set_bind_group(0, bind, &[]);
                    pass.set_vertex_buffer(0, vbuf.slice(..));
                    pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*count, 0, 0..1);
                }
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
