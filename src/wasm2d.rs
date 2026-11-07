//! Browser entry for 2D: WebGL2 sprite drawing and frame-row playback.
//! Quads are drawn in call order with nearest sampling.

#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
// The deny above resets the crate's choices for these, so they are restated.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]

use crate::sprite_anim::{Facing, RowEvent, RowMode, SpriteAnimator, SpriteRow};
use wasm_bindgen::prelude::*;
use web_sys::{
    HtmlCanvasElement, HtmlImageElement, WebGl2RenderingContext as Gl, WebGlBuffer, WebGlProgram,
    WebGlShader, WebGlTexture,
};

const VS: &str = "#version 300 es
in vec2 p;in vec2 u;in vec4 c;out vec2 vu;out vec4 vc;uniform vec2 s;void main(){vu=u;vc=c;gl_Position=vec4(p.x/s.x*2.-1.,1.-p.y/s.y*2.,0,1);}";
const FS: &str = "#version 300 es
precision mediump float;in vec2 vu;in vec4 vc;uniform sampler2D t;out vec4 o;void main(){vec4 x=texture(t,vu)*vc;if(x.a<.01)discard;o=x;}";
const STRIDE: i32 = 32;

fn shader(g: &Gl, kind: u32, src: &str) -> Result<WebGlShader, JsError> {
    let h = g
        .create_shader(kind)
        .ok_or_else(|| JsError::new("create_shader"))?;
    g.shader_source(&h, src);
    g.compile_shader(&h);
    if g.get_shader_parameter(&h, Gl::COMPILE_STATUS).as_bool() == Some(true) {
        Ok(h)
    } else {
        Err(JsError::new(&g.get_shader_info_log(&h).unwrap_or_default()))
    }
}

/// Draws textured quads and filled rectangles on a WebGL2 canvas.
#[wasm_bindgen]
pub struct Renderer2D {
    g: Gl,
    program: WebGlProgram,
    vbo: WebGlBuffer,
    textures: Vec<WebGlTexture>,
    verts: Vec<f32>,
    bound: u32,
}

#[wasm_bindgen]
impl Renderer2D {
    /// Takes over `canvas`.
    ///
    /// # Errors
    /// Fails if WebGL2 is unavailable or the shaders do not build.
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: &HtmlCanvasElement) -> Result<Self, JsError> {
        let opts = js_sys_opts();
        let g: Gl = canvas
            .get_context_with_context_options("webgl2", &opts)
            .map_err(|_| JsError::new("getContext"))?
            .ok_or_else(|| JsError::new("WebGL2 not available"))?
            .dyn_into()
            .map_err(|_| JsError::new("not WebGL2"))?;
        let program = g
            .create_program()
            .ok_or_else(|| JsError::new("create_program"))?;
        g.attach_shader(&program, &shader(&g, Gl::VERTEX_SHADER, VS)?);
        g.attach_shader(&program, &shader(&g, Gl::FRAGMENT_SHADER, FS)?);
        g.link_program(&program);
        if g.get_program_parameter(&program, Gl::LINK_STATUS).as_bool() != Some(true) {
            return Err(JsError::new(
                &g.get_program_info_log(&program).unwrap_or_default(),
            ));
        }
        g.use_program(Some(&program));
        g.uniform2f(
            g.get_uniform_location(&program, "s").as_ref(),
            canvas.width() as f32,
            canvas.height() as f32,
        );
        let vbo = g
            .create_buffer()
            .ok_or_else(|| JsError::new("create_buffer"))?;
        g.bind_buffer(Gl::ARRAY_BUFFER, Some(&vbo));
        for (name, size, off) in [("p", 2, 0), ("u", 2, 8), ("c", 4, 16)] {
            let l = g.get_attrib_location(&program, name);
            if l < 0 {
                return Err(JsError::new("attribute missing"));
            }
            g.enable_vertex_attrib_array(l as u32);
            g.vertex_attrib_pointer_with_i32(l as u32, size, Gl::FLOAT, false, STRIDE, off);
        }
        g.enable(Gl::BLEND);
        g.blend_func(Gl::SRC_ALPHA, Gl::ONE_MINUS_SRC_ALPHA);
        let mut r = Self {
            g,
            program,
            vbo,
            textures: Vec::new(),
            verts: Vec::new(),
            bound: 0,
        };
        r.white()?;
        Ok(r)
    }

    fn params(&self) {
        let g = &self.g;
        for (k, v) in [
            (Gl::TEXTURE_MIN_FILTER, Gl::NEAREST),
            (Gl::TEXTURE_MAG_FILTER, Gl::NEAREST),
            (Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE),
            (Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE),
        ] {
            g.tex_parameteri(Gl::TEXTURE_2D, k, v.cast_signed());
        }
    }

    fn white(&mut self) -> Result<(), JsError> {
        let t = self
            .g
            .create_texture()
            .ok_or_else(|| JsError::new("create_texture"))?;
        self.g.bind_texture(Gl::TEXTURE_2D, Some(&t));
        self.g
            .tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                Gl::TEXTURE_2D,
                0,
                Gl::RGBA.cast_signed(),
                1,
                1,
                0,
                Gl::RGBA,
                Gl::UNSIGNED_BYTE,
                Some(&[255, 255, 255, 255]),
            )
            .map_err(|_| JsError::new("texImage2D"))?;
        self.params();
        self.textures.push(t);
        Ok(())
    }

    /// Uploads an image and returns its id for [`Self::sprite`].
    ///
    /// # Errors
    /// Fails if the texture cannot be created or the image cannot be uploaded.
    pub fn texture(&mut self, image: &HtmlImageElement) -> Result<u32, JsError> {
        self.flush();
        let t = self
            .g
            .create_texture()
            .ok_or_else(|| JsError::new("create_texture"))?;
        self.g.bind_texture(Gl::TEXTURE_2D, Some(&t));
        self.g
            .tex_image_2d_with_u32_and_u32_and_html_image_element(
                Gl::TEXTURE_2D,
                0,
                Gl::RGBA.cast_signed(),
                Gl::RGBA,
                Gl::UNSIGNED_BYTE,
                image,
            )
            .map_err(|_| JsError::new("texImage2D"))?;
        self.params();
        self.textures.push(t);
        self.bound = u32::MAX;
        Ok(self.textures.len() as u32 - 1)
    }

    /// Clears to an opaque colour.
    pub fn clear(&mut self, r: f32, g: f32, b: f32) {
        self.flush();
        self.g.clear_color(r, g, b, 1.0);
        self.g.clear(Gl::COLOR_BUFFER_BIT);
    }

    fn select(&mut self, tex: u32) {
        if self.bound != tex {
            self.flush();
            self.bound = tex;
        }
    }

    fn push(&mut self, x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], c: [f32; 4]) {
        let [u0, v0, u1, v1] = uv;
        for (px, py, u, v) in [
            (x, y, u0, v0),
            (x + w, y, u1, v0),
            (x, y + h, u0, v1),
            (x + w, y, u1, v0),
            (x + w, y + h, u1, v1),
            (x, y + h, u0, v1),
        ] {
            self.verts
                .extend_from_slice(&[px, py, u, v, c[0], c[1], c[2], c[3]]);
        }
    }

    /// A filled rectangle.
    #[allow(clippy::too_many_arguments)]
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, g: f32, b: f32, a: f32) {
        self.select(0);
        self.push(x, y, w, h, [0.0, 0.0, 1.0, 1.0], [r, g, b, a]);
    }

    /// A textured quad. Swap `u0` and `u1` to mirror.
    #[allow(clippy::too_many_arguments)]
    pub fn sprite(
        &mut self,
        tex: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        u0: f32,
        v0: f32,
        u1: f32,
        v1: f32,
        a: f32,
    ) {
        self.select(tex);
        self.push(x, y, w, h, [u0, v0, u1, v1], [1.0, 1.0, 1.0, a]);
    }

    /// Sends what is queued. Call at the end of each frame.
    pub fn flush(&mut self) {
        if self.verts.is_empty() {
            return;
        }
        let g = &self.g;
        if let Some(t) = self.textures.get(self.bound as usize) {
            g.bind_texture(Gl::TEXTURE_2D, Some(t));
        }
        // Safety: the view is used before any allocation can move wasm memory.
        unsafe {
            let view = js_sys::Float32Array::view(&self.verts);
            g.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &view, Gl::DYNAMIC_DRAW);
        }
        // 4 GiB of wasm memory holds under 2^28 vertices, so the count fits.
        #[allow(clippy::cast_possible_wrap)]
        g.draw_arrays(Gl::TRIANGLES, 0, (self.verts.len() / 8) as i32);
        self.verts.clear();
    }

    /// Releases the GL objects. The renderer draws nothing afterwards.
    pub fn destroy(&mut self) {
        self.verts.clear();
        for t in self.textures.drain(..) {
            self.g.delete_texture(Some(&t));
        }
        self.g.delete_buffer(Some(&self.vbo));
        self.g.delete_program(Some(&self.program));
    }
}

fn js_sys_opts() -> JsValue {
    let o = js_sys::Object::new();
    for (k, v) in [
        ("alpha", false),
        ("antialias", false),
        ("preserveDrawingBuffer", true),
    ] {
        let _ = js_sys::Reflect::set(&o, &k.into(), &v.into());
    }
    o.into()
}

/// Frame-row playback for the page.
#[wasm_bindgen(js_name = SpriteAnimator)]
#[derive(Default)]
pub struct WasmSpriteAnimator {
    inner: SpriteAnimator,
}

#[wasm_bindgen(js_class = SpriteAnimator)]
impl WasmSpriteAnimator {
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a row. `order[i]` is shown for `ms[i]`.
    ///
    /// # Errors
    /// Refuses a row the animator refuses: see [`SpriteAnimator::add_row`].
    #[wasm_bindgen(js_name = addRow)]
    pub fn add_row(
        &mut self,
        name: &str,
        order: Vec<u16>,
        ms: Vec<u32>,
        looping: bool,
    ) -> Result<(), JsError> {
        let mode = if looping {
            RowMode::Loop
        } else {
            RowMode::Once
        };
        self.inner
            .add_row(name, SpriteRow { order, ms, mode })
            .map_err(|e| JsError::new(&format!("{e:?}")))
    }

    /// Starts a row. The current row keeps its place.
    ///
    /// # Errors
    /// Refuses a name that was not added.
    pub fn play(&mut self, name: &str) -> Result<(), JsError> {
        self.inner
            .play(name)
            .map_err(|e| JsError::new(&format!("{e:?}")))
    }

    /// Returns true on the call that finishes a non-looping row.
    pub fn advance(&mut self, ms: u32) -> bool {
        self.inner.advance(ms) == Some(RowEvent::Finished)
    }

    /// Chooses the frame from the position in whole units, so a planted foot
    /// stays put. Returns the position to draw the figure at.
    ///
    /// # Errors
    /// Refuses a zero step, or a call before the first play.
    #[wasm_bindgen(js_name = seekByDistance)]
    pub fn seek_by_distance(&mut self, x: i32, step: u32, left: bool) -> Result<f64, JsError> {
        let facing = if left { Facing::Left } else { Facing::Right };
        self.inner
            .seek_by_distance(i64::from(x), step, facing)
            .map(|snapped| snapped as f64)
            .map_err(|e| JsError::new(&format!("{e:?}")))
    }

    /// The frame number to draw, or undefined before the first play.
    #[must_use]
    pub fn frame(&self) -> Option<u16> {
        self.inner.frame()
    }

    /// The position within the row's shown frames.
    // wasm_bindgen does not take a const fn.
    #[allow(clippy::missing_const_for_fn)]
    #[must_use]
    pub fn position(&self) -> u32 {
        self.inner.position() as u32
    }
}
