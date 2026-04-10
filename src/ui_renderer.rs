//! Minimal UI overlay renderer — colored rectangles over the 3D scene.
//!
//! Draws UiContext widgets as 2D colored quads in a second render pass
//! (LoadOp::Load to preserve the underlying scene).
//!
//! Currently renders: Panel (dark bg), Slider (bar), Checkbox (square),
//! Label (small text placeholder), Button (colored rect), ProgressBar.
//! Actual text rendering is not yet implemented — labels show as colored bars.

#![cfg(feature = "window")]

use crate::ui::{UiContext, WidgetKind};

/// A colored 2D quad in clip-space coordinates [-1, 1].
#[derive(Debug, Clone, Copy)]
struct Quad {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    g: f32,
    b: f32,
}

impl Quad {
    /// Convert to 4 vertices (position2 + color3) + 6 indices.
    fn to_verts(&self, base_idx: u16) -> ([f32; 20], [u16; 6]) {
        let x0 = self.x;
        let y0 = self.y;
        let x1 = self.x + self.w;
        let y1 = self.y + self.h;
        let (r, g, b) = (self.r, self.g, self.b);

        let verts = [
            x0, y0, r, g, b, // top-left
            x1, y0, r, g, b, // top-right
            x1, y1, r, g, b, // bottom-right
            x0, y1, r, g, b, // bottom-left
        ];
        let idx = [
            base_idx,
            base_idx + 1,
            base_idx + 2,
            base_idx,
            base_idx + 2,
            base_idx + 3,
        ];
        (verts, idx)
    }
}

/// Build a list of quads from the UiContext.
///
/// Widgets are stacked vertically in the top-left corner.
pub fn build_ui_quads(ui: &UiContext) -> (Vec<f32>, Vec<u16>) {
    let mut all_verts: Vec<f32> = Vec::new();
    let mut all_indices: Vec<u16> = Vec::new();

    // Layout: stack widgets top-left, each 0.3 wide, 0.05 tall in clip space
    let x_start = -0.95;
    let mut y = 0.90;
    let w = 0.35;
    let h = 0.045;
    let gap = 0.005;

    let widget_count = ui.widget_count();
    for i in 0..widget_count {
        let widget_id = crate::ui::WidgetId(i as u32);
        let widget = match ui.get(widget_id) {
            Some(w) => w,
            None => continue,
        };
        if !widget.visible {
            continue;
        }

        let quads = match &widget.kind {
            WidgetKind::Panel => {
                vec![Quad {
                    x: x_start - 0.02,
                    y: y + 0.02,
                    w: w + 0.04,
                    h: -(h * 6.0 + gap * 5.0 + 0.04),
                    r: 0.1,
                    g: 0.1,
                    b: 0.15,
                }]
            }
            WidgetKind::Label { text, .. } => {
                // 文字の長さに応じたバー
                let bar_w = (text.len() as f32 * 0.012).min(w);
                let q = Quad {
                    x: x_start,
                    y,
                    w: bar_w,
                    h: -h,
                    r: 0.7,
                    g: 0.7,
                    b: 0.7,
                };
                y -= h + gap;
                vec![q]
            }
            WidgetKind::Slider { value, min, max } => {
                let range = max - min;
                let fill = if range > 0.0 {
                    ((value - min) / range).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let bg = Quad {
                    x: x_start,
                    y,
                    w,
                    h: -h,
                    r: 0.2,
                    g: 0.2,
                    b: 0.25,
                };
                let bar = Quad {
                    x: x_start,
                    y,
                    w: w * fill,
                    h: -h,
                    r: 0.3,
                    g: 0.7,
                    b: 0.3,
                };
                y -= h + gap;
                vec![bg, bar]
            }
            WidgetKind::Checkbox { checked, .. } => {
                let size = h;
                let bg = Quad {
                    x: x_start,
                    y,
                    w: size,
                    h: -size,
                    r: 0.3,
                    g: 0.3,
                    b: 0.35,
                };
                let mut q = vec![bg];
                if *checked {
                    q.push(Quad {
                        x: x_start + 0.005,
                        y: y - 0.005,
                        w: size - 0.01,
                        h: -(size - 0.01),
                        r: 0.2,
                        g: 0.8,
                        b: 0.2,
                    });
                }
                y -= h + gap;
                q
            }
            WidgetKind::Button { .. } => {
                let q = Quad {
                    x: x_start,
                    y,
                    w,
                    h: -h,
                    r: 0.3,
                    g: 0.4,
                    b: 0.7,
                };
                y -= h + gap;
                vec![q]
            }
            WidgetKind::ProgressBar { value } => {
                let bg = Quad {
                    x: x_start,
                    y,
                    w,
                    h: -h,
                    r: 0.2,
                    g: 0.2,
                    b: 0.25,
                };
                let bar = Quad {
                    x: x_start,
                    y,
                    w: w * value.clamp(0.0, 1.0),
                    h: -h,
                    r: 0.8,
                    g: 0.5,
                    b: 0.2,
                };
                y -= h + gap;
                vec![bg, bar]
            }
            _ => continue,
        };

        for q in quads {
            let base = (all_verts.len() / 5) as u16;
            let (v, i) = q.to_verts(base);
            all_verts.extend_from_slice(&v);
            all_indices.extend_from_slice(&i);
        }
    }

    (all_verts, all_indices)
}

/// Render UI quads as a 2D overlay (call after the 3D scene pass).
///
/// `surface_view` is the texture view of the current frame. The caller
/// is responsible for presenting afterwards.
pub fn render_ui_overlay(
    device: &wgpu::Device,
    _queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    surface_format: wgpu::TextureFormat,
    ui: &UiContext,
) {
    let (verts, indices) = build_ui_quads(ui);
    if indices.is_empty() {
        return;
    }

    let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ui_vb"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ui_ib"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let shader_src = r"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) color: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.pos = vec4<f32>(position, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(@location(0) color: vec3<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(color, 0.85);
}
";

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("ui_shader"),
        source: wgpu::ShaderSource::Wgsl(shader_src.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("ui_pipeline_layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("ui_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 20, // 2f position + 3f color = 5 * 4
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        offset: 8,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x3,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui_overlay"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load, // 3D scene を保持
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
    }
}

use wgpu::util::DeviceExt;
