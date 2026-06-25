//! Virtual shadow map caster demo — opens a depth-only render pass
//! scoped to one atlas page via [`VirtualShadowGpu::render_caster_to_page`],
//! draws a fullscreen triangle that writes a fixed depth value into
//! that page, then reads back the central texel and asserts the
//! depth landed in the right page slot.
//!
//! ```bash
//! cargo run --example virtual_shadow_demo --features gpu
//! ```

use alice_game_engine::virtual_shadow::{
    PhysicalPageHandle, VirtualShadowConfig, VirtualShadowGpu, VirtualShadowMap,
};

const DEPTH_FRAGMENT_WGSL: &str = r"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    var out: VsOut;
    out.pos = vec4<f32>(pos[vi], 0.42, 1.0);
    return out;
}

@fragment
fn fs_main() {}
";

fn main() {
    println!("=== Virtual Shadow Caster Demo ===");

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    })) {
        Ok(a) => a,
        Err(e) => {
            println!("no compatible adapter ({e:?}); skipping");
            return;
        }
    };
    println!("adapter: {}", adapter.get_info().name);

    let (device, queue) =
        match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("alice-vshadow-demo"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        })) {
            Ok(p) => p,
            Err(e) => {
                println!("device creation failed ({e:?}); skipping");
                return;
            }
        };

    // 4×4 page atlas, 64 px per page → 256×256 atlas, 16 physical pages.
    let mut cpu_map = VirtualShadowMap::new(VirtualShadowConfig {
        page_size: 64,
        virtual_pages: 4,
        physical_pages: 16,
    });
    let vshadow = VirtualShadowGpu::new(&device, 4, 64);
    let page = cpu_map
        .allocate(alice_game_engine::virtual_shadow::VirtualPageId { x: 2, y: 1, mip: 0 })
        .unwrap();
    let (vx, vy, vw, vh) = vshadow.page_viewport(page);
    println!(
        "page handle index = {}, viewport = ({vx}, {vy}, {vw}, {vh})",
        page.index,
    );

    // Build a tiny depth-only pipeline (no color target).
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("vshadow-depth-shader"),
        source: wgpu::ShaderSource::Wgsl(DEPTH_FRAGMENT_WGSL.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vshadow-pipeline-layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("vshadow-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[],
        }),
        multiview: None,
        cache: None,
    });

    // Render the fullscreen triangle into exactly this page.
    vshadow.render_caster_to_page(&device, &queue, page, 1.0, |pass| {
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    });

    // Read back the centre texel of the page; expect ~0.42.
    let edge = vshadow.atlas_pages_per_side * vshadow.page_size;
    let bytes_per_pixel = 4_u32; // Depth32Float
    let row_bytes = edge * bytes_per_pixel;
    let total_size = (row_bytes * edge) as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vshadow-readback"),
        size: total_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("vshadow-readback-enc"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &vshadow.atlas_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::DepthOnly,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_bytes),
                rows_per_image: Some(edge),
            },
        },
        wgpu::Extent3d {
            width: edge,
            height: edge,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));

    use std::sync::mpsc;
    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::Wait).unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = slice.get_mapped_range();

    // Sample the centre of our page and an outside page.
    let centre_x = (vx + vw * 0.5) as u32;
    let centre_y = (vy + vh * 0.5) as u32;
    let centre_idx = (centre_y * row_bytes + centre_x * bytes_per_pixel) as usize;
    let centre = f32::from_le_bytes([
        bytes[centre_idx],
        bytes[centre_idx + 1],
        bytes[centre_idx + 2],
        bytes[centre_idx + 3],
    ]);

    // Sample a different page's centre (last page, far from page 0).
    let outside_idx = ((edge - 32) * row_bytes + (edge - 32) * bytes_per_pixel) as usize;
    let outside = f32::from_le_bytes([
        bytes[outside_idx],
        bytes[outside_idx + 1],
        bytes[outside_idx + 2],
        bytes[outside_idx + 3],
    ]);

    drop(bytes);
    readback.unmap();
    let _ = PhysicalPageHandle { index: 0 };

    println!("centre texel (= inside page): {centre:.3} (expected ≈ 0.420)");
    println!("outside texel (= different page): {outside:.3} (expected ≈ 1.000 untouched)");
    if (centre - 0.42).abs() < 0.05 && outside > 0.5 {
        println!("page-scoped depth write verified — viewport restriction works.");
    } else {
        println!("WARNING: page targeting did not produce the expected depth values");
    }
}
