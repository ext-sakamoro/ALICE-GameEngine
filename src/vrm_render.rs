//! Drawing a posed VRM primitive into an offscreen image with a two-tone (toon) shader.
//!
//! The vertices come already posed ([`crate::vrm_pose::VrmPose::posed_positions`]), so the
//! skinning and the morph targets are the CPU's, one implementation for both the image and
//! anything that hashes or compares the pose. The shader is `MToon` 1.0's shading:
//! `s = dot(normal, light) + shift`, `t = clamp((s + 1 - toony) / (2 - 2 toony), 0, 1)` (a step at
//! `s = 0` when toony is 1), colour = `mix(shade, base, t)`. A part may carry a [`FacePaint`]:
//! 2D shapes (ellipses and strokes) on a plane in front of the face, evaluated per fragment as
//! signed distances, so eyes, brows and a mouth are laws on the surface rather than meshes, and
//! an expression is a change of the shapes' numbers. A part may also carry a [`Surface`]: UVs, a
//! base-colour and a shade texture (multiplying the base and shade colours, as `MToon` 1.0's
//! `baseColorTexture` and `shadeMultiplyTexture`), a UV transform (`KHR_texture_transform`) and
//! an alpha mode (opaque, mask with a cutoff, or blend). [`toon_wgsl`] is plain text (validated
//! by naga in the tests); the renderer is behind the `gpu` feature.
//!
//! [`ToonTerms`] add, per part, what a toon material may carry beyond that: culling of the back
//! faces, and a [`Rim`] light, where the surface turns away from the viewer:
//! `f = pow(1 - |n . v|, power)`, a ramp from `border - blur / 2` to `border + blur / 2` (widened
//! by `fwidth(f) * antialias`), times the lit factor by the shadow mask, coloured by the rim colour
//! (times a mask texture, and mixed toward colour * base by the albedo mix) and added in linear
//! light. The outline may take the base colour, unshaded, in place of its lighting mix. Without
//! terms a part is drawn as before: both faces, no rim, the outline's lighting mix.
//!
//! The renderer draws one sample per pixel unless told 4 ([`ToonRenderer::set_samples`]):
//! multisampled edges, the fragment shaded once per pixel and the samples averaged.

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
    // x: base texture present, y: shade texture present, z: alpha mode (0 opaque, 1 mask,
    // 2 blend), w: the mask's cutoff
    surface: vec4<f32>,
    // xy: UV offset, zw: UV scale
    uv_a: vec4<f32>,
    // x: UV rotation (radians)
    uv_b: vec4<f32>,
    // the inverse of view_proj (a fragment's view ray)
    inv_view_proj: mat4x4<f32>,
    // xy: the image size in pixels
    viewport: vec4<f32>,
    // rim: rgb colour, a strength
    rim_color: vec4<f32>,
    // x: Fresnel power, y: border, z: blur, w: shadow mask (0..=1)
    rim: vec4<f32>,
    // x: albedo mix, y: antialias, z: 1 = no rim on back faces, w: 1 = rim on
    rim_b: vec4<f32>,
    // x: 1 = the outline colour times the base colour (unshaded) in place of the lighting mix
    outline_b: vec4<f32>,
    // world -> face plane: xy are the paint coordinates, z the depth along the face's forward
    to_face: mat4x4<f32>,
    // x: the number of shapes, y: the least face depth that is painted
    paint: vec4<f32>,
    // 3 vec4 per shape: (kind, p0.x, p0.y, p1.x), (p1.y, p2.x, p2.y, half width), colour
    shapes: array<vec4<f32>, 96>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var base_tex: texture_2d<f32>;
@group(0) @binding(2) var shade_tex: texture_2d<f32>;
@group(0) @binding(3) var samp: sampler;
@group(0) @binding(4) var rim_tex: texture_2d<f32>;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = params.view_proj * vec4<f32>(v.position, 1.0);
    out.normal = v.normal;
    out.world = v.position;
    out.uv = v.uv;
    return out;
}

// KHR_texture_transform: offset + rotation(scale * uv), the rotation as the extension writes it
fn uv_of(uv: vec2<f32>) -> vec2<f32> {
    let s = uv * params.uv_a.zw;
    let c = cos(params.uv_b.x);
    let n = sin(params.uv_b.x);
    return params.uv_a.xy + vec2<f32>(c * s.x + n * s.y, -n * s.x + c * s.y);
}

// the lit and the shaded colour of a fragment: the colours times their textures (always sampled,
// in uniform control flow; a part without a texture samples a white texel)
fn colours(uv: vec2<f32>) -> array<vec4<f32>, 2> {
    let t = uv_of(uv);
    let b = textureSample(base_tex, samp, t);
    let h = textureSample(shade_tex, samp, t);
    let base = params.base_color * select(vec4<f32>(1.0), b, params.surface.x > 0.5);
    let shade = params.shade_color * select(vec4<f32>(1.0), h, params.surface.y > 0.5);
    return array<vec4<f32>, 2>(base, shade);
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
    out.uv = v.uv;
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

// the lit factor at a normal (MToon 1.0 shading): 0 in shade, 1 lit
fn lit_factor(normal: vec3<f32>) -> f32 {
    let s = dot(normalize(normal), normalize(params.light.xyz)) + params.light.w;
    let toony = params.shading.x;
    let width = 2.0 - 2.0 * toony;
    if (width <= 0.0) {
        return select(0.0, 1.0, s >= 0.0);
    }
    return clamp((s + 1.0 - toony) / width, 0.0, 1.0);
}

// the surface's toon colour at a normal between its shaded and lit colours
fn toon(normal: vec3<f32>, c: array<vec4<f32>, 2>) -> vec4<f32> {
    let rgb = mix(c[1].rgb, c[0].rgb, lit_factor(normal));
    return vec4<f32>(rgb, c[0].a);
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3<f32>(0.0031308));
}

// 1 - |n . v| to the Fresnel power, v the direction from the fragment toward the viewer
fn rim_fresnel(f: VsOut) -> f32 {
    let ndc = vec2<f32>(2.0 * f.clip.x / params.viewport.x - 1.0, 1.0 - 2.0 * f.clip.y / params.viewport.y);
    let near = params.inv_view_proj * vec4<f32>(ndc, 0.0, 1.0);
    let v = normalize(near.xyz / near.w - f.world);
    return pow(clamp(1.0 - abs(dot(normalize(f.normal), v)), 0.0, 1.0), params.rim.x);
}

// the colour with the rim added in linear light (unchanged without a rim, or on a back face when
// the rim skips them)
fn add_rim(rgb: vec3<f32>, albedo: vec3<f32>, fres: f32, fw: f32, m: vec4<f32>, lit: f32, front: bool) -> vec3<f32> {
    if (params.rim_b.w < 0.5 || (params.rim_b.z > 0.5 && !front)) {
        return rgb;
    }
    let lo = clamp(params.rim.y - params.rim.z * 0.5, 0.0, 1.0);
    let hi = clamp(params.rim.y + params.rim.z * 0.5, 0.0, 1.0);
    var r = clamp((fres - lo) / max(clamp(hi - lo + fw * params.rim_b.y, 0.0, 1.0), 0.0001), 0.0, 1.0);
    r = mix(r, r * lit, params.rim.w);
    var col = to_linear(params.rim_color.rgb) * to_linear(m.rgb);
    col = mix(col, col * to_linear(albedo), params.rim_b.x);
    return to_srgb(to_linear(rgb) + col * (params.rim_color.a * m.a * r));
}

// the outline colour, tinted by the surface's lit colour by the lighting mix
@fragment
fn fs_outline(f: VsOut) -> @location(0) vec4<f32> {
    let c0 = colours(f.uv);
    if (params.surface.z > 0.5 && params.surface.z < 1.5 && c0[0].a < params.surface.w) {
        discard;
    }
    let lit = toon(f.normal, c0).rgb;
    var c = params.outline_color.rgb * mix(vec3<f32>(1.0, 1.0, 1.0), lit, params.outline.z);
    if (params.outline_b.x > 0.5) {
        c = params.outline_color.rgb * c0[0].rgb;
    }
    return vec4<f32>(c, 1.0);
}

@fragment
fn fs_main(f: VsOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let c0 = colours(f.uv);
    // sampled and differentiated in uniform control flow, before any discard
    let rim_mask = textureSample(rim_tex, samp, uv_of(f.uv));
    let fres = rim_fresnel(f);
    let fw = fwidth(fres);
    // mask: below the cutoff the fragment is not drawn at all
    if (params.surface.z > 0.5 && params.surface.z < 1.5 && c0[0].a < params.surface.w) {
        discard;
    }
    var color = toon(f.normal, c0);
    color = vec4<f32>(add_rim(color.rgb, c0[0].rgb, fres, fw, rim_mask, lit_factor(f.normal), front), color.a);
    // opaque and mask draw solid; blend keeps the alpha
    if (params.surface.z < 1.5) {
        color.a = 1.0;
    }
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

/// An RGBA8 image a part samples: `width` x `height` texels, rows top first (as decoded), four
/// bytes each. The bytes are taken as they are (no sRGB conversion), as the colours are
#[derive(Debug, Clone, Copy)]
pub struct ToonTexture<'a> {
    /// Texels across (1..=8192)
    pub width: u32,
    /// Texels down (1..=8192)
    pub height: u32,
    /// `width * height * 4` bytes
    pub rgba: &'a [u8],
    /// The caller's name for exactly these bytes, if any: a renderer uploads a keyed texture once
    /// and draws later parts of the same key from that upload (until `forget_textures`), so the
    /// same key must never name different bytes. `None` uploads at every draw
    pub key: Option<u64>,
}

/// A UV transform (`KHR_texture_transform`): `offset + rotation(scale * uv)`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvTransform {
    /// Added last
    pub offset: [f32; 2],
    /// Multiplies the UV first
    pub scale: [f32; 2],
    /// Radians, as the extension writes it
    pub rotation: f32,
}

impl UvTransform {
    /// No change
    pub const IDENTITY: Self = Self {
        offset: [0.0, 0.0],
        scale: [1.0, 1.0],
        rotation: 0.0,
    };
}

/// How a part's alpha is drawn
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlphaMode {
    /// Solid
    Opaque,
    /// Solid where the alpha is at least the cutoff (0..=1), not drawn elsewhere
    Mask(f32),
    /// Mixed over what is behind it by its alpha; drawn after the solid parts, in their order,
    /// without writing depth
    Blend,
}

/// A part's textured surface
#[derive(Debug, Clone, Copy)]
pub struct Surface<'a> {
    /// One UV per vertex
    pub uvs: &'a [[f32; 2]],
    /// Multiplies the base colour, if any
    pub base: Option<ToonTexture<'a>>,
    /// Multiplies the shade colour, if any
    pub shade: Option<ToonTexture<'a>>,
    /// Applied to the UVs before sampling
    pub transform: UvTransform,
    /// How the base colour's alpha is drawn
    pub alpha: AlphaMode,
}

/// Checks a surface against its part's vertex count: one finite UV per vertex, each texture of
/// a size in 1..=8192 with exactly its bytes, a finite transform and a mask cutoff in 0..=1
///
/// # Errors
///
/// [`RenderError::BadSurface`]
pub fn check_surface(s: &Surface<'_>, vertices: usize) -> Result<(), RenderError> {
    if s.uvs.len() != vertices {
        return Err(RenderError::BadSurface(format!(
            "{} uvs for {vertices} vertices",
            s.uvs.len()
        )));
    }
    if !s.uvs.iter().flatten().all(|x| x.is_finite()) {
        return Err(RenderError::BadSurface("a uv that is not finite".into()));
    }
    for (name, t) in [("base", s.base), ("shade", s.shade)] {
        let Some(t) = t else { continue };
        if !(1..=8192).contains(&t.width) || !(1..=8192).contains(&t.height) {
            return Err(RenderError::BadSurface(format!(
                "{name} texture {} x {}",
                t.width, t.height
            )));
        }
        if t.rgba.len() as u64 != u64::from(t.width) * u64::from(t.height) * 4 {
            return Err(RenderError::BadSurface(format!(
                "{name} texture {} x {} with {} bytes",
                t.width,
                t.height,
                t.rgba.len()
            )));
        }
    }
    let tr = s.transform;
    if !tr
        .offset
        .iter()
        .chain(&tr.scale)
        .chain([&tr.rotation])
        .all(|x| x.is_finite())
    {
        return Err(RenderError::BadSurface(
            "a uv transform that is not finite".into(),
        ));
    }
    if let AlphaMode::Mask(c) = s.alpha {
        if !(0.0..=1.0).contains(&c) {
            return Err(RenderError::BadSurface(format!("mask cutoff {c}")));
        }
    }
    Ok(())
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> u8 {
    let s = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055f32.mul_add(c.powf(1.0 / 2.4), -0.055)
    };
    // in 0..=255 after the clamp
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let v = (s.clamp(0.0, 1.0) * 255.0).round() as u8;
    v
}

/// The mip chain of an RGBA8 image (sRGB colour, linear alpha), level 0 first and every level
/// after it, down to 1 x 1: each level's texel is the mean of the 2 x 2 texels above it (the
/// last row or column repeated where a size is odd), the colour averaged in linear light.
/// Returns the bytes of all levels in order and the number of levels
#[must_use]
pub fn mip_chain(rgba: &[u8], width: u32, height: u32) -> (Vec<u8>, u32) {
    let mut out = rgba.to_vec();
    let (mut w, mut h) = (width as usize, height as usize);
    let mut level: Vec<[f32; 4]> = rgba
        .chunks_exact(4)
        .map(|p| {
            [
                srgb_to_linear(p[0]),
                srgb_to_linear(p[1]),
                srgb_to_linear(p[2]),
                f32::from(p[3]) / 255.0,
            ]
        })
        .collect();
    let mut levels = 1;
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![[0.0f32; 4]; nw * nh];
        for y in 0..nh {
            for x in 0..nw {
                let mut acc = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let sx = (2 * x + dx).min(w - 1);
                    let sy = (2 * y + dy).min(h - 1);
                    let t = level[sy * w + sx];
                    for c in 0..4 {
                        acc[c] += t[c] * 0.25;
                    }
                }
                next[y * nw + x] = acc;
            }
        }
        for t in &next {
            out.extend_from_slice(&[
                linear_to_srgb(t[0]),
                linear_to_srgb(t[1]),
                linear_to_srgb(t[2]),
                // in 0..=255 after the clamp
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    (t[3].clamp(0.0, 1.0) * 255.0).round() as u8
                },
            ]);
        }
        level = next;
        w = nw;
        h = nh;
        levels += 1;
    }
    (out, levels)
}

/// The uniform words of a surface (after the outline): presence, alpha mode and cutoff, then
/// the UV transform
fn surface_words(s: Option<&Surface<'_>>) -> [f32; 12] {
    let Some(s) = s else {
        return [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    };
    let (mode, cutoff) = match s.alpha {
        AlphaMode::Opaque => (0.0, 0.0),
        AlphaMode::Mask(c) => (1.0, c),
        AlphaMode::Blend => (2.0, 0.0),
    };
    let flag = |t: Option<ToonTexture<'_>>| if t.is_some() { 1.0 } else { 0.0 };
    let tr = s.transform;
    [
        flag(s.base),
        flag(s.shade),
        mode,
        cutoff,
        tr.offset[0],
        tr.offset[1],
        tr.scale[0],
        tr.scale[1],
        tr.rotation,
        0.0,
        0.0,
        0.0,
    ]
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
    /// The part's textured surface, if any (without one: the colours alone, opaque)
    pub surface: Option<Surface<'a>>,
}

/// A rim light: where the surface turns away from the viewer (see the module doc)
#[derive(Debug, Clone, Copy)]
pub struct Rim<'a> {
    /// rgb: the colour (sRGB, as a material stores it), a: the strength
    pub color: [f32; 4],
    /// The power of `1 - |n . v|` (> 0)
    pub fresnel_power: f32,
    /// The centre of the ramp
    pub border: f32,
    /// The width of the ramp (>= 0)
    pub blur: f32,
    /// How much `fwidth` widens the ramp (>= 0; 0 is none)
    pub antialias: f32,
    /// 0..=1: 0 the rim ignores shade, 1 the rim is times the lit factor
    pub shadow_mask: f32,
    /// 0..=1: 0 the rim colour as it is, 1 the rim colour times the base colour
    pub albedo_mix: f32,
    /// No rim on back faces
    pub skip_back_faces: bool,
    /// Multiplies the colour and the strength, sampled at the surface's UVs (needs a surface)
    pub mask: Option<ToonTexture<'a>>,
}

/// What a part carries beyond [`ToonPart`] (the default: both faces drawn, no rim)
#[derive(Debug, Clone, Copy, Default)]
pub struct ToonTerms<'a> {
    /// Back faces are not drawn (glTF's `doubleSided: false`)
    pub cull_back: bool,
    /// A rim light, if any
    pub rim: Option<Rim<'a>>,
    /// The outline's colour is its colour times the base colour (with its texture), unshaded,
    /// in place of the outline's lighting mix
    pub outline_times_base: bool,
    /// The textures are read at their full size only (no smaller mip level): for a texture that
    /// is data (a colour strip indexed by UV), where a mip level would blend unrelated texels
    pub base_level_only: bool,
}

/// Checks a part's terms: a rim's numbers finite, its power positive, its blur and antialias not
/// negative, its shadow mask and albedo mix in 0..=1, and its mask texture of a size in 1..=8192
/// with exactly its bytes, on a part with a surface
///
/// # Errors
///
/// [`RenderError::BadTerms`]
pub fn check_terms(t: &ToonTerms<'_>, has_surface: bool) -> Result<(), RenderError> {
    let Some(r) = t.rim else { return Ok(()) };
    let nums = [
        r.fresnel_power,
        r.border,
        r.blur,
        r.antialias,
        r.shadow_mask,
        r.albedo_mix,
    ];
    if !r.color.iter().chain(&nums).all(|x| x.is_finite()) {
        return Err(RenderError::BadTerms(
            "a rim number that is not finite".into(),
        ));
    }
    if r.fresnel_power <= 0.0 {
        return Err(RenderError::BadTerms(format!(
            "rim power {}",
            r.fresnel_power
        )));
    }
    if r.blur < 0.0 || r.antialias < 0.0 {
        return Err(RenderError::BadTerms(format!(
            "rim blur {} antialias {}",
            r.blur, r.antialias
        )));
    }
    if !(0.0..=1.0).contains(&r.shadow_mask) || !(0.0..=1.0).contains(&r.albedo_mix) {
        return Err(RenderError::BadTerms(format!(
            "rim shadow mask {} albedo mix {}",
            r.shadow_mask, r.albedo_mix
        )));
    }
    if let Some(m) = r.mask {
        if !has_surface {
            return Err(RenderError::BadTerms(
                "a rim mask on a part without a surface".into(),
            ));
        }
        if !(1..=8192).contains(&m.width) || !(1..=8192).contains(&m.height) {
            return Err(RenderError::BadTerms(format!(
                "rim mask {} x {}",
                m.width, m.height
            )));
        }
        if m.rgba.len() as u64 != u64::from(m.width) * u64::from(m.height) * 4 {
            return Err(RenderError::BadTerms(format!(
                "rim mask {} x {} with {} bytes",
                m.width,
                m.height,
                m.rgba.len()
            )));
        }
    }
    Ok(())
}

/// The uniform words of the terms (after the surface): the inverse view-projection, the image
/// size, the rim, then the outline's base flag
fn terms_words(t: &ToonTerms<'_>, view_proj: [f32; 16], width: f32, height: f32) -> Vec<f32> {
    let mut w = glam::Mat4::from_cols_array(&view_proj)
        .inverse()
        .to_cols_array()
        .to_vec();
    w.extend_from_slice(&[width, height, 0.0, 0.0]);
    match t.rim {
        None => w.extend_from_slice(&[0.0; 12]),
        Some(r) => {
            w.extend_from_slice(&r.color);
            w.extend_from_slice(&[r.fresnel_power, r.border, r.blur, r.shadow_mask]);
            w.extend_from_slice(&[
                r.albedo_mix,
                r.antialias,
                if r.skip_back_faces { 1.0 } else { 0.0 },
                1.0,
            ]);
        }
    }
    w.extend_from_slice(&[if t.outline_times_base { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0]);
    w
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
    /// A surface that [`check_surface`] refuses
    BadSurface(String),
    /// Terms that [`check_terms`] refuses, or not one per part
    BadTerms(String),
    /// A sample count other than 1 or 4
    BadSamples(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDevice(e) => write!(f, "no GPU device: {e}"),
            Self::BadMesh(e) => write!(f, "bad mesh: {e}"),
            Self::Readback(e) => write!(f, "readback failed: {e}"),
            Self::BadPaint(e) => write!(f, "bad paint: {e}"),
            Self::BadOutline(e) => write!(f, "bad outline: {e}"),
            Self::BadSurface(e) => write!(f, "bad surface: {e}"),
            Self::BadTerms(e) => write!(f, "bad terms: {e}"),
            Self::BadSamples(e) => write!(f, "bad samples: {e}"),
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
        check_mesh, check_outline, check_paint, check_surface, check_terms, paint_words,
        surface_words, terms_words, toon_wgsl, AlphaMode, OutlineMode, RenderError, ToonParams,
        ToonPart, ToonTerms, ToonTexture,
    };
    use wgpu::util::DeviceExt;

    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
    const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    /// An offscreen toon renderer of one size
    pub struct ToonRenderer {
        device: wgpu::Device,
        queue: wgpu::Queue,
        pipeline: wgpu::RenderPipeline,
        blend_pipeline: wgpu::RenderPipeline,
        cull_pipeline: wgpu::RenderPipeline,
        cull_blend_pipeline: wgpu::RenderPipeline,
        outline_pipeline: wgpu::RenderPipeline,
        shader: wgpu::ShaderModule,
        pipeline_layout: wgpu::PipelineLayout,
        samples: u32,
        sampler: wgpu::Sampler,
        base_sampler: wgpu::Sampler,
        layout: wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        textures: std::sync::Mutex<std::collections::HashMap<u64, wgpu::Texture>>,
    }

    /// A pipeline of the toon shader with these entry points, culling `cull`; `blend` mixes by
    /// alpha and does not write depth; `format` is the colour target's
    #[allow(clippy::too_many_arguments)]
    fn toon_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        vs: &str,
        fs: &str,
        cull: Option<wgpu::Face>,
        blend: bool,
        format: wgpu::TextureFormat,
        samples: u32,
    ) -> wgpu::RenderPipeline {
        let vertex_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(vs),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(vs),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &vertex_attrs,
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fs),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: blend.then_some(wgpu::BlendState::ALPHA_BLENDING),
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
                depth_write_enabled: !blend,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                ..wgpu::MultisampleState::default()
            },
            multiview: None,
            cache: None,
        })
    }

    /// The five pipelines (solid, blended, both culled, outline) at `samples` per pixel
    fn toon_pipelines(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        pl: &wgpu::PipelineLayout,
        format: wgpu::TextureFormat,
        samples: u32,
    ) -> [wgpu::RenderPipeline; 5] {
        let back = Some(wgpu::Face::Back);
        let main = |cull, blend| {
            toon_pipeline(
                device, shader, pl, "vs_main", "fs_main", cull, blend, format, samples,
            )
        };
        // the hull's front faces would cover the surface; its back faces show around it
        let outline = toon_pipeline(
            device,
            shader,
            pl,
            "vs_outline",
            "fs_outline",
            Some(wgpu::Face::Front),
            false,
            format,
            samples,
        );
        [
            main(None, false),
            main(None, true),
            main(back, false),
            main(back, true),
            outline,
        ]
    }

    const fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }
    }

    impl ToonRenderer {
        /// An RGBA8 texture of the image, uploaded as it is (rows top first)
        fn texture(&self, t: ToonTexture<'_>) -> wgpu::Texture {
            let Some(key) = t.key else {
                return self.upload(t);
            };
            let mut cache = self
                .textures
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cache.entry(key).or_insert_with(|| self.upload(t)).clone()
        }

        /// Drops the keyed textures, so a key can name new bytes
        pub fn forget_textures(&self) {
            self.textures
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
        }

        /// Uploads the image with its mip chain ([`super::mip_chain`])
        fn upload(&self, t: ToonTexture<'_>) -> wgpu::Texture {
            let (data, levels) = super::mip_chain(t.rgba, t.width, t.height);
            self.device.create_texture_with_data(
                &self.queue,
                &wgpu::TextureDescriptor {
                    label: Some("vrm toon texture"),
                    size: wgpu::Extent3d {
                        width: t.width,
                        height: t.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: levels,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: FORMAT,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &data,
            )
        }

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
                    // the downlevel limits, but textures as large as the adapter allows (a
                    // character's textures are often 4096 across)
                    required_limits: wgpu::Limits {
                        max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d,
                        ..wgpu::Limits::downlevel_defaults()
                    },
                    memory_hints: wgpu::MemoryHints::default(),
                    trace: wgpu::Trace::Off,
                }))
                .map_err(|e| RenderError::NoDevice(e.to_string()))?;
            Self::with_device(device, queue, FORMAT, width, height)
        }

        /// A renderer on a device the caller made (a browser's, a window's), drawing
        /// `width` x `height` images of `format` (`draw_parts_into` a view of that format;
        /// `draw_parts` reads back only from `Rgba8Unorm`). The device needs textures as large as
        /// the parts', and the downlevel limits otherwise
        ///
        /// # Errors
        ///
        /// [`RenderError::BadMesh`] for a size outside 1..=8192
        pub fn with_device(
            device: wgpu::Device,
            queue: wgpu::Queue,
            format: wgpu::TextureFormat,
            width: u32,
            height: u32,
        ) -> Result<Self, RenderError> {
            if width == 0 || height == 0 || width > 8192 || height > 8192 {
                return Err(RenderError::BadMesh(format!(
                    "image size {width} x {height}"
                )));
            }
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("vrm toon"),
                source: wgpu::ShaderSource::Wgsl(toon_wgsl().into()),
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("vrm toon params"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    texture_entry(1),
                    texture_entry(2),
                    texture_entry(4),
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vrm toon"),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
            let [pipeline, blend_pipeline, cull_pipeline, cull_blend_pipeline, outline_pipeline] =
                toon_pipelines(&device, &shader, &pl, format, 1);
            // linear filtering within the nearest mip level, repeating (UVs outside 0..1 wrap, as
            // glTF's default sampler)
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("vrm toon"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..wgpu::SamplerDescriptor::default()
            });
            // the same at the full-size level only
            let base_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("vrm toon base level"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                lod_max_clamp: 0.0,
                ..wgpu::SamplerDescriptor::default()
            });
            Ok(Self {
                device,
                queue,
                pipeline,
                blend_pipeline,
                cull_pipeline,
                cull_blend_pipeline,
                outline_pipeline,
                shader,
                pipeline_layout: pl,
                samples: 1,
                sampler,
                base_sampler,
                layout,
                format,
                width,
                height,
                textures: std::sync::Mutex::new(std::collections::HashMap::new()),
            })
        }

        /// Draws later images with `samples` per pixel: 1, or 4 (multisampled, resolved into the
        /// image)
        ///
        /// # Errors
        ///
        /// [`RenderError::BadSamples`] for any other count
        pub fn set_samples(&mut self, samples: u32) -> Result<(), RenderError> {
            if samples != 1 && samples != 4 {
                return Err(RenderError::BadSamples(format!(
                    "{samples} samples, 1 or 4"
                )));
            }
            if samples != self.samples {
                let [a, b, c, d, e] = toon_pipelines(
                    &self.device,
                    &self.shader,
                    &self.pipeline_layout,
                    self.format,
                    samples,
                );
                self.pipeline = a;
                self.blend_pipeline = b;
                self.cull_pipeline = c;
                self.cull_blend_pipeline = d;
                self.outline_pipeline = e;
                self.samples = samples;
            }
            Ok(())
        }

        /// The samples per pixel images are drawn with
        #[must_use]
        pub const fn samples(&self) -> u32 {
            self.samples
        }

        /// Draws later images `width` x `height` (1..=8192 each)
        ///
        /// # Errors
        ///
        /// [`RenderError::BadMesh`] for a size outside 1..=8192
        pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
            if width == 0 || height == 0 || width > 8192 || height > 8192 {
                return Err(RenderError::BadMesh(format!(
                    "image size {width} x {height}"
                )));
            }
            self.width = width;
            self.height = height;
            Ok(())
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
                    surface: None,
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
        pub fn draw_parts(
            &self,
            parts: &[ToonPart<'_>],
            clear: [f32; 4],
        ) -> Result<Vec<u8>, RenderError> {
            self.draw_parts_terms(parts, &[], clear)
        }

        /// As `draw_parts`, each part with its [`ToonTerms`] (one per part; none: the defaults)
        ///
        /// # Errors
        ///
        /// As `draw_parts`, and [`RenderError::BadTerms`]
        pub fn draw_parts_terms(
            &self,
            parts: &[ToonPart<'_>],
            terms: &[ToonTerms<'_>],
            clear: [f32; 4],
        ) -> Result<Vec<u8>, RenderError> {
            if self.format != FORMAT {
                return Err(RenderError::Readback(format!(
                    "reading back {:?}, only {FORMAT:?} is read",
                    self.format
                )));
            }
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
            let tview = target.create_view(&wgpu::TextureViewDescriptor::default());
            let mut enc = self.encode(parts, terms, clear, &tview)?;
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

        /// Draws the parts as `draw_parts` does into `view` (a `width` x `height` view of the
        /// renderer's format, a browser canvas or a window surface) and submits the work
        ///
        /// # Errors
        ///
        /// As `draw_parts`, without the readback
        pub fn draw_parts_into(
            &self,
            parts: &[ToonPart<'_>],
            clear: [f32; 4],
            view: &wgpu::TextureView,
        ) -> Result<(), RenderError> {
            self.draw_parts_terms_into(parts, &[], clear, view)
        }

        /// As `draw_parts_into`, each part with its [`ToonTerms`] (one per part; none: the
        /// defaults)
        ///
        /// # Errors
        ///
        /// As `draw_parts_into`, and [`RenderError::BadTerms`]
        pub fn draw_parts_terms_into(
            &self,
            parts: &[ToonPart<'_>],
            terms: &[ToonTerms<'_>],
            clear: [f32; 4],
            view: &wgpu::TextureView,
        ) -> Result<(), RenderError> {
            let enc = self.encode(parts, terms, clear, view)?;
            self.queue.submit(Some(enc.finish()));
            Ok(())
        }

        #[allow(clippy::too_many_lines)]
        fn encode(
            &self,
            parts: &[ToonPart<'_>],
            terms: &[ToonTerms<'_>],
            clear: [f32; 4],
            tview: &wgpu::TextureView,
        ) -> Result<wgpu::CommandEncoder, RenderError> {
            if parts.is_empty() {
                return Err(RenderError::BadMesh("no part to draw".into()));
            }
            if !terms.is_empty() && terms.len() != parts.len() {
                return Err(RenderError::BadTerms(format!(
                    "{} terms for {} parts",
                    terms.len(),
                    parts.len()
                )));
            }
            let default_terms = ToonTerms::default();
            let term_of = |i: usize| terms.get(i).unwrap_or(&default_terms);
            let max_tex = self.device.limits().max_texture_dimension_2d;
            for (i, part) in parts.iter().enumerate() {
                if let Some(s) = &part.surface {
                    for t in [s.base, s.shade].into_iter().flatten() {
                        if t.width > max_tex || t.height > max_tex {
                            return Err(RenderError::BadSurface(format!(
                                "part {i}: texture {} x {} past this device's {max_tex}",
                                t.width, t.height
                            )));
                        }
                    }
                }
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
                if let Some(surface) = &part.surface {
                    check_surface(surface, part.positions.len()).map_err(|e| match e {
                        RenderError::BadSurface(m) => {
                            RenderError::BadSurface(format!("part {i}: {m}"))
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
                check_terms(term_of(i), part.surface.is_some()).map_err(|e| match e {
                    RenderError::BadTerms(m) => RenderError::BadTerms(format!("part {i}: {m}")),
                    other => other,
                })?;
                if let Some(m) = term_of(i).rim.and_then(|r| r.mask) {
                    if m.width > max_tex || m.height > max_tex {
                        return Err(RenderError::BadTerms(format!(
                            "part {i}: rim mask {} x {} past this device's {max_tex}",
                            m.width, m.height
                        )));
                    }
                }
            }
            let uploads: Vec<_> = parts
                .iter()
                .enumerate()
                .map(|(i, part)| {
                    let p = &part.params;
                    let term = term_of(i);
                    let uvs = part.surface.as_ref().map(|s| s.uvs);
                    let verts: Vec<f32> = part
                        .positions
                        .iter()
                        .zip(part.normals)
                        .enumerate()
                        .flat_map(|(k, (a, n))| {
                            let uv = uvs.map_or([0.0, 0.0], |u| u[k]);
                            a.iter().chain(n).chain(&uv).copied().collect::<Vec<f32>>()
                        })
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
                    uni.extend_from_slice(&surface_words(part.surface.as_ref()));
                    #[allow(clippy::cast_precision_loss)]
                    let (w, h) = (self.width as f32, self.height as f32);
                    uni.extend(terms_words(term, p.view_proj, w, h));
                    uni.extend(paint_words(part.paint));
                    let ubuf = self
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("vrm toon params"),
                            contents: bytemuck::cast_slice(&uni),
                            usage: wgpu::BufferUsages::UNIFORM,
                        });
                    let white = ToonTexture {
                        width: 1,
                        height: 1,
                        rgba: &[255, 255, 255, 255],
                        key: None,
                    };
                    let surf = part.surface.as_ref();
                    let base = self.texture(surf.and_then(|s| s.base).unwrap_or(white));
                    let shade = self.texture(surf.and_then(|s| s.shade).unwrap_or(white));
                    let rim = self.texture(term.rim.and_then(|r| r.mask).unwrap_or(white));
                    let bv = base.create_view(&wgpu::TextureViewDescriptor::default());
                    let sv = shade.create_view(&wgpu::TextureViewDescriptor::default());
                    let rv = rim.create_view(&wgpu::TextureViewDescriptor::default());
                    let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("vrm toon params"),
                        layout: &self.layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: ubuf.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&bv),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&sv),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::Sampler(if term.base_level_only {
                                    &self.base_sampler
                                } else {
                                    &self.sampler
                                }),
                            },
                            wgpu::BindGroupEntry {
                                binding: 4,
                                resource: wgpu::BindingResource::TextureView(&rv),
                            },
                        ],
                    });
                    #[allow(clippy::cast_possible_truncation)]
                    let count = part.indices.len() as u32;
                    let outlined = part.outline.is_some_and(|o| o.width > 0.0);
                    let blended = surf.is_some_and(|s| s.alpha == AlphaMode::Blend);
                    (vbuf, ibuf, bind, count, outlined, blended, term.cull_back)
                })
                .collect();
            let size = wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            };
            // with several samples: a multisampled colour target, resolved into the view
            let msaa = (self.samples > 1).then(|| {
                self.device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("vrm toon multisampled"),
                        size,
                        mip_level_count: 1,
                        sample_count: self.samples,
                        dimension: wgpu::TextureDimension::D2,
                        format: self.format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .create_view(&wgpu::TextureViewDescriptor::default())
            });
            let depth = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vrm toon depth"),
                size,
                mip_level_count: 1,
                sample_count: self.samples,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
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
                        view: msaa.as_ref().unwrap_or(tview),
                        resolve_target: msaa.as_ref().map(|_| tview),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                            store: if msaa.is_some() {
                                wgpu::StoreOp::Discard
                            } else {
                                wgpu::StoreOp::Store
                            },
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
                for (vbuf, ibuf, bind, count, outlined, _, _) in &uploads {
                    if *outlined {
                        pass.set_bind_group(0, bind, &[]);
                        pass.set_vertex_buffer(0, vbuf.slice(..));
                        pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..*count, 0, 0..1);
                    }
                }
                // the solid parts, then the blended ones in their order over them
                for want_blend in [false, true] {
                    for (vbuf, ibuf, bind, count, _, blended, cull) in &uploads {
                        if *blended != want_blend {
                            continue;
                        }
                        pass.set_pipeline(match (want_blend, *cull) {
                            (false, false) => &self.pipeline,
                            (true, false) => &self.blend_pipeline,
                            (false, true) => &self.cull_pipeline,
                            (true, true) => &self.cull_blend_pipeline,
                        });
                        pass.set_bind_group(0, bind, &[]);
                        pass.set_vertex_buffer(0, vbuf.slice(..));
                        pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..*count, 0, 0..1);
                    }
                }
            }
            Ok(enc)
        }
    }
}
