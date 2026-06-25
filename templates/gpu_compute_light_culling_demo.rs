//! GPU compute headless demo — exercises the tiled light culling
//! compute shader on a real GPU device. Pair with
//! `gpu_compute_demo` (which targets the DDGI update shader).
//!
//! ```bash
//! cargo run --example gpu_compute_light_culling_demo --features gpu
//! ```

use alice_game_engine::light_culling::TiledLightCullerGpu;
use alice_game_engine::math::{Mat4, Vec3};

fn main() {
    println!("=== GPU Compute Headless Demo (light_culling) ===");

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
            println!("no compatible adapter ({e:?}); skipping GPU demo");
            return;
        }
    };
    println!("adapter: {}", adapter.get_info().name);

    let (device, queue) =
        match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("alice-light-culling-demo"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        })) {
            Ok(p) => p,
            Err(e) => {
                println!("device creation failed ({e:?}); skipping GPU demo");
                return;
            }
        };

    // Tiny scene: 4 point lights at known positions.
    let lights: [(Vec3, f32); 4] = [
        (Vec3::new(0.0, 0.0, 0.0), 4.0),
        (Vec3::new(3.0, 0.0, 0.0), 2.0),
        (Vec3::new(-3.0, 0.0, 0.0), 2.0),
        (Vec3::new(0.0, 2.0, -2.0), 5.0),
    ];
    let light_count = lights.len() as u32;

    let screen_w: u32 = 320;
    let screen_h: u32 = 180;
    let tile_size: u32 = 16;
    let tile_count_x = screen_w.div_ceil(tile_size);
    let tile_count_y = screen_h.div_ceil(tile_size);
    let total_tiles = tile_count_x * tile_count_y;
    let max_lights_per_tile: u32 = 8;

    println!(
        "screen {screen_w}x{screen_h}, tile {tile_size}, tiles {}x{} ({total_tiles}), lights {light_count}",
        tile_count_x, tile_count_y,
    );

    // Camera looking down -Z from (0, 0, 5).
    let view = Mat4::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    let projection = Mat4::perspective(std::f32::consts::FRAC_PI_3, 16.0 / 9.0, 0.1, 100.0);

    let gpu = TiledLightCullerGpu::new(&device);

    // Build the uniform buffer: 2 mat4 + 7 u32 + 1 pad = 32 floats + 8 ints.
    use wgpu::util::DeviceExt;
    let view_cols = view.0.to_cols_array();
    let proj_cols = projection.0.to_cols_array();
    let mut uniform_bytes: Vec<u8> = Vec::with_capacity(160);
    for f in view_cols.iter().chain(proj_cols.iter()) {
        uniform_bytes.extend_from_slice(&f.to_ne_bytes());
    }
    for v in [
        screen_w,
        screen_h,
        tile_size,
        tile_count_x,
        tile_count_y,
        light_count,
        max_lights_per_tile,
        0u32, // pad
    ] {
        uniform_bytes.extend_from_slice(&v.to_ne_bytes());
    }
    let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("light-cull-uniform"),
        contents: &uniform_bytes,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    // Storage buffer for the lights (vec3 + radius packed as 4 floats).
    let mut light_bytes: Vec<u8> = Vec::with_capacity(light_count as usize * 16);
    for (pos, radius) in lights.iter() {
        light_bytes.extend_from_slice(&pos.x().to_ne_bytes());
        light_bytes.extend_from_slice(&pos.y().to_ne_bytes());
        light_bytes.extend_from_slice(&pos.z().to_ne_bytes());
        light_bytes.extend_from_slice(&radius.to_ne_bytes());
    }
    let lights_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("light-cull-lights"),
        contents: &light_bytes,
        usage: wgpu::BufferUsages::STORAGE,
    });

    // Per-tile atomic counter.
    let counts_size = (total_tiles as u64) * 4;
    let counts_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("light-cull-counts"),
        size: counts_size,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    // Per-tile light index list, flat.
    let indices_size = (total_tiles as u64) * (max_lights_per_tile as u64) * 4;
    let indices_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("light-cull-indices"),
        size: indices_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("light-cull-bind"),
        layout: &gpu.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: lights_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: counts_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: indices_buf.as_entire_binding(),
            },
        ],
    });

    // Encode + dispatch + readback.
    use std::sync::mpsc;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("alice-light-cull-encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("alice-light-cull-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&gpu.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(TiledLightCullerGpu::workgroup_count_x(light_count), 1, 1);
    }
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("light-cull-readback"),
        size: counts_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_buffer_to_buffer(&counts_buf, 0, &readback, 0, counts_size);
    queue.submit(std::iter::once(encoder.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::Wait).unwrap();
    rx.recv().unwrap().unwrap();
    let data = slice.get_mapped_range();
    let counts: &[u32] = bytemuck::cast_slice(&data);
    let max_count = *counts.iter().max().unwrap_or(&0);
    let nonzero_tiles = counts.iter().filter(|c| **c > 0).count();
    drop(data);
    readback.unmap();

    println!(
        "tile counts: max per-tile = {max_count}, tiles covered = {nonzero_tiles} / {total_tiles}",
    );
    if nonzero_tiles > 0 {
        println!("compute pipeline assigned lights to ≥1 tile — GPU dispatch verified.");
    } else {
        println!("WARNING: no tiles received any light — projection / view may be off-camera");
    }
}
