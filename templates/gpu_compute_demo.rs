//! GPU compute headless demo — exercises the DDGI update compute
//! shader end-to-end on a real GPU device. Uses pollster to drive
//! the wgpu Instance + Adapter + Device without a window, then
//! dispatches the compute pipeline via
//! [`GpuContext::dispatch_compute_once`].
//!
//! ```bash
//! cargo run --example gpu_compute_demo --features gpu
//! ```
//!
//! If no compatible GPU adapter is available (e.g. CI headless
//! Linux) the demo logs a friendly skip message and exits 0.

use alice_game_engine::ddgi::{DdgiConfig, DdgiVolumeGpu};

fn main() {
    println!("=== GPU Compute Headless Demo (DDGI update) ===");

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
            label: Some("alice-gpu-compute-demo"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        })) {
            Ok(pair) => pair,
            Err(e) => {
                println!("device creation failed ({e:?}); skipping GPU demo");
                return;
            }
        };

    // Build pipeline + tiny probe volume (4 probes × 2×2 octahedron = 48 RGB floats).
    let cfg = DdgiConfig {
        grid: (2, 1, 2),
        irradiance_resolution: 2,
        hysteresis: 1.0,
        ..DdgiConfig::default()
    };
    let probe_count: u32 = cfg.grid.0 * cfg.grid.1 * cfg.grid.2;
    let texels_per_probe = (cfg.irradiance_resolution * cfg.irradiance_resolution) as usize;
    let channels_per_probe = texels_per_probe * 3;
    let total_floats = probe_count as usize * channels_per_probe;

    let gpu_volume = DdgiVolumeGpu::new(&device);

    // Uniform buffer: probe_count, irradiance_resolution, hysteresis, pad.
    use wgpu::util::DeviceExt;
    let uniform_bytes = [
        probe_count.to_ne_bytes(),
        cfg.irradiance_resolution.to_ne_bytes(),
        cfg.hysteresis.to_bits().to_ne_bytes(),
        0u32.to_ne_bytes(),
    ]
    .concat();
    let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ddgi-uniform"),
        contents: &uniform_bytes,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    // Samples: every channel = 0.7 (= warm grey).
    let samples: Vec<f32> = vec![0.7; total_floats];
    let samples_bytes: Vec<u8> = samples.iter().flat_map(|f| f.to_ne_bytes()).collect();
    let samples_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ddgi-samples"),
        contents: &samples_bytes,
        usage: wgpu::BufferUsages::STORAGE,
    });

    // Irradiance: start at zero, will be overwritten by the compute.
    let irradiance_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ddgi-irradiance"),
        size: (total_floats * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ddgi-bind"),
        layout: &gpu_volume.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: samples_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: irradiance_buf.as_entire_binding(),
            },
        ],
    });

    // Use the gpu module's one-shot dispatch helper (read back the
    // irradiance buffer to verify the compute actually wrote).
    use std::sync::mpsc;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("alice-gpu-compute-demo-encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("alice-gpu-compute-demo-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&gpu_volume.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        let (gx, gy, gz) = DdgiVolumeGpu::workgroup_count(probe_count);
        pass.dispatch_workgroups(gx, gy, gz);
    }
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ddgi-readback"),
        size: (total_floats * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_buffer_to_buffer(&irradiance_buf, 0, &readback, 0, (total_floats * 4) as u64);
    queue.submit(std::iter::once(encoder.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::Wait).unwrap();
    rx.recv().unwrap().unwrap();
    let data = slice.get_mapped_range();
    let floats: &[f32] = bytemuck::cast_slice(&data);

    let max = floats.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let min = floats.iter().copied().fold(f32::INFINITY, f32::min);
    let mean: f32 = floats.iter().sum::<f32>() / floats.len() as f32;
    drop(data);
    readback.unmap();

    println!("probes: {probe_count}, irradiance floats: {total_floats}, expected ≈ 0.7");
    println!("min: {min:.4}, max: {max:.4}, mean: {mean:.4}");
    if (mean - 0.7).abs() < 0.05 {
        println!("compute pipeline produced the expected value — GPU dispatch verified.");
    } else {
        println!("WARNING: expected mean ≈ 0.7 (samples filled with 0.7) but got {mean:.4}");
    }
}
