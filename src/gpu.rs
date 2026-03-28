//! GPU context: wgpu Device, Queue, Surface initialization, and `GBuffer`
//! texture management.

#[cfg(feature = "gpu")]
use wgpu;

use crate::math::Color;

// ---------------------------------------------------------------------------
// GpuConfig
// ---------------------------------------------------------------------------

/// Configuration for GPU initialization.
#[derive(Debug, Clone)]
pub struct GpuConfig {
    pub power_preference: GpuPowerPreference,
    pub present_mode: PresentMode,
    pub sample_count: u32,
}

/// GPU power preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuPowerPreference {
    LowPower,
    HighPerformance,
}

/// Present mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentMode {
    Fifo,
    Mailbox,
    Immediate,
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            power_preference: GpuPowerPreference::HighPerformance,
            present_mode: PresentMode::Fifo,
            sample_count: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// TextureFormat
// ---------------------------------------------------------------------------

/// Texture format identifiers matching the `GBuffer` layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    Rgba8Unorm,
    Rgba8UnormSrgb,
    Rgb10a2Unorm,
    R8Uint,
    Depth32Float,
    Bgra8UnormSrgb,
}

// ---------------------------------------------------------------------------
// GBufferTextures — CPU-side descriptor
// ---------------------------------------------------------------------------

/// Describes the `GBuffer` texture set (without actual GPU resources).
#[derive(Debug, Clone)]
pub struct GBufferTextures {
    pub width: u32,
    pub height: u32,
    pub albedo_format: TextureFormat,
    pub normal_format: TextureFormat,
    pub emission_format: TextureFormat,
    pub material_format: TextureFormat,
    pub depth_format: TextureFormat,
}

impl GBufferTextures {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            albedo_format: TextureFormat::Rgba8UnormSrgb,
            normal_format: TextureFormat::Rgb10a2Unorm,
            emission_format: TextureFormat::Rgba8Unorm,
            material_format: TextureFormat::Rgba8Unorm,
            depth_format: TextureFormat::Depth32Float,
        }
    }

    #[must_use]
    pub const fn pixel_count(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// Estimated VRAM usage in bytes (5 attachments × 4 bytes per pixel).
    #[must_use]
    pub const fn estimated_vram_bytes(&self) -> u64 {
        self.pixel_count() * 5 * 4
    }

    pub const fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }
}

// ---------------------------------------------------------------------------
// GpuContext — wgpu wrapper (runtime initialization)
// ---------------------------------------------------------------------------

/// Holds wgpu Device, Queue, and surface configuration.
/// Created at runtime when a window is available.
#[cfg(feature = "gpu")]
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub gbuffer: GBufferTextures,
}

#[cfg(feature = "gpu")]
impl GpuContext {
    /// Creates a `GpuContext` from an existing wgpu surface.
    /// Call this with `pollster::block_on`.
    ///
    /// # Errors
    ///
    /// Returns an error string if adapter or device creation fails.
    pub async fn from_surface(
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
        config: &GpuConfig,
    ) -> Result<Self, String> {
        let power = match config.power_preference {
            GpuPowerPreference::LowPower => wgpu::PowerPreference::LowPower,
            GpuPowerPreference::HighPerformance => wgpu::PowerPreference::HighPerformance,
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: power,
                compatible_surface: Some(surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("No suitable GPU adapter: {e}"))?;

        let (device, queue): (wgpu::Device, wgpu::Queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("ALICE GPU Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| format!("Device creation failed: {e}"))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: match config.present_mode {
                PresentMode::Fifo => wgpu::PresentMode::Fifo,
                PresentMode::Mailbox => wgpu::PresentMode::Mailbox,
                PresentMode::Immediate => wgpu::PresentMode::Immediate,
            },
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        Ok(Self {
            device,
            queue,
            surface_config,
            gbuffer: GBufferTextures::new(width, height),
        })
    }

    /// Resizes the surface and `GBuffer`.
    pub fn resize(&mut self, surface: &wgpu::Surface<'_>, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            surface.configure(&self.device, &self.surface_config);
            self.gbuffer.resize(width, height);
        }
    }

    /// Returns the surface texture format.
    #[must_use]
    pub const fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_config.format
    }

    /// Creates a shader module from WGSL source.
    #[must_use]
    pub fn create_shader(&self, label: &str, wgsl: &str) -> wgpu::ShaderModule {
        self.device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(wgsl.into()),
            })
    }

    /// Creates a vertex+index buffer pair from raw bytes.
    #[must_use]
    pub fn create_mesh_buffers(
        &self,
        vertices: &[u8],
        indices: &[u8],
    ) -> (wgpu::Buffer, wgpu::Buffer) {
        use wgpu::util::DeviceExt;
        let vb = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertex_buffer"),
                contents: vertices,
                usage: wgpu::BufferUsages::VERTEX,
            });
        let ib = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("index_buffer"),
                contents: indices,
                usage: wgpu::BufferUsages::INDEX,
            });
        (vb, ib)
    }

    /// Creates a uniform buffer.
    #[must_use]
    pub fn create_uniform_buffer(&self, data: &[u8]) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("uniform_buffer"),
                contents: data,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })
    }

    /// Writes data to an existing buffer.
    pub fn write_buffer(&self, buffer: &wgpu::Buffer, data: &[u8]) {
        self.queue.write_buffer(buffer, 0, data);
    }

    /// Creates a `wgpu::Texture` from raw RGBA8 pixel data.
    #[must_use]
    pub fn create_texture_rgba8(
        &self,
        width: u32,
        height: u32,
        data: &[u8],
        label: &str,
    ) -> (wgpu::Texture, wgpu::TextureView, wgpu::Sampler) {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        (texture, view, sampler)
    }

    /// Renders a single frame: acquire surface → clear with color → present.
    /// Returns `Ok(())` on success, `Err` if the surface is lost.
    ///
    /// # Errors
    ///
    /// Returns an error string on surface acquisition failure.
    pub fn render_clear(
        &self,
        surface: &wgpu::Surface<'_>,
        clear_color: Color,
    ) -> Result<(), String> {
        let output = surface
            .get_current_texture()
            .map_err(|e| format!("Surface error: {e}"))?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let [r, g, b, a] = clear_color_to_array(clear_color);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("clear_encoder"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    /// Renders a frame with a render pipeline and vertex/index buffers.
    ///
    /// # Errors
    ///
    /// Returns an error string on surface acquisition failure.
    #[allow(clippy::too_many_arguments)]
    pub fn render_mesh(
        &self,
        surface: &wgpu::Surface<'_>,
        pipeline: &wgpu::RenderPipeline,
        vertex_buffer: &wgpu::Buffer,
        index_buffer: &wgpu::Buffer,
        index_count: u32,
        bind_group: &wgpu::BindGroup,
        clear_color: Color,
    ) -> Result<(), String> {
        let output = surface
            .get_current_texture()
            .map_err(|e| format!("Surface error: {e}"))?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let [r, g, b, a] = clear_color_to_array(clear_color);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mesh_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..index_count, 0, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Fullscreen triangle (for post-processing)
// ---------------------------------------------------------------------------

/// Vertices for a fullscreen triangle (covers clip space -1..1).
/// Uses 3 vertices without index buffer.
pub const FULLSCREEN_TRIANGLE_POSITIONS: [[f32; 2]; 3] = [[-1.0, -1.0], [3.0, -1.0], [-1.0, 3.0]];

/// UVs for fullscreen triangle.
pub const FULLSCREEN_TRIANGLE_UVS: [[f32; 2]; 3] = [[0.0, 1.0], [2.0, 1.0], [0.0, -1.0]];

// ---------------------------------------------------------------------------
// ClearColor helper
// ---------------------------------------------------------------------------

/// Converts engine Color to wgpu-compatible clear values.
#[must_use]
pub fn clear_color_to_array(color: Color) -> [f64; 4] {
    [
        f64::from(color.r),
        f64::from(color.g),
        f64::from(color.b),
        f64::from(color.a),
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_config_default() {
        let cfg = GpuConfig::default();
        assert_eq!(cfg.power_preference, GpuPowerPreference::HighPerformance);
        assert_eq!(cfg.present_mode, PresentMode::Fifo);
        assert_eq!(cfg.sample_count, 1);
    }

    #[test]
    fn gbuffer_textures_new() {
        let gb = GBufferTextures::new(1920, 1080);
        assert_eq!(gb.width, 1920);
        assert_eq!(gb.height, 1080);
        assert_eq!(gb.depth_format, TextureFormat::Depth32Float);
    }

    #[test]
    fn gbuffer_pixel_count() {
        let gb = GBufferTextures::new(1920, 1080);
        assert_eq!(gb.pixel_count(), 1920 * 1080);
    }

    #[test]
    fn gbuffer_estimated_vram() {
        let gb = GBufferTextures::new(1920, 1080);
        // 1920*1080 * 5 * 4 = ~41MB
        assert!(gb.estimated_vram_bytes() > 40_000_000);
    }

    #[test]
    fn gbuffer_resize() {
        let mut gb = GBufferTextures::new(800, 600);
        gb.resize(1920, 1080);
        assert_eq!(gb.width, 1920);
        assert_eq!(gb.height, 1080);
    }

    #[test]
    fn fullscreen_triangle() {
        assert_eq!(FULLSCREEN_TRIANGLE_POSITIONS.len(), 3);
        assert_eq!(FULLSCREEN_TRIANGLE_UVS.len(), 3);
    }

    #[test]
    fn clear_color_conversion() {
        let c = Color::new(1.0, 0.5, 0.0, 1.0);
        let arr = clear_color_to_array(c);
        assert!((arr[0] - 1.0).abs() < 1e-6);
        assert!((arr[1] - 0.5).abs() < 1e-3);
    }

    #[test]
    fn texture_formats() {
        let f = TextureFormat::Rgba8Unorm;
        assert_ne!(f, TextureFormat::Depth32Float);
    }

    #[test]
    fn power_preference_variants() {
        assert_ne!(
            GpuPowerPreference::LowPower,
            GpuPowerPreference::HighPerformance
        );
    }

    #[test]
    fn present_mode_variants() {
        let modes = [
            PresentMode::Fifo,
            PresentMode::Mailbox,
            PresentMode::Immediate,
        ];
        assert_eq!(modes.len(), 3);
    }
}
