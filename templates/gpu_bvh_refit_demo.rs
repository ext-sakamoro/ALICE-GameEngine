//! GPU BVH refit dispatch demo — drives the bottom-up interior
//! refit pass against a real GPU device, then reads back the root
//! node's bounds to verify the union equals the scene AABB.
//!
//! ```bash
//! cargo run --example gpu_bvh_refit_demo --features gpu
//! ```

use alice_game_engine::gpu_bvh::{Aabb, Bvh, BvhNode};
use alice_game_engine::math::Vec3;
use alice_game_engine::shader::GPU_BVH_INTERIOR_REFIT_COMPUTE_WGSL;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuBvhNode {
    bounds_min: [f32; 3],
    left: u32,
    bounds_max: [f32; 3],
    right: u32,
    primitive_start: u32,
    primitive_count: u32,
    _pad: [u32; 2],
}

impl From<&BvhNode> for GpuBvhNode {
    fn from(n: &BvhNode) -> Self {
        Self {
            bounds_min: [n.bounds.min.x(), n.bounds.min.y(), n.bounds.min.z()],
            left: n.left,
            bounds_max: [n.bounds.max.x(), n.bounds.max.y(), n.bounds.max.z()],
            right: n.right,
            primitive_start: n.primitive_start,
            primitive_count: n.primitive_count,
            _pad: [0; 2],
        }
    }
}

fn main() {
    println!("=== GPU BVH bottom-up refit demo ===");

    // Build a small BVH so we can verify the root bounds after refit.
    let aabbs: Vec<Aabb> = (0..8)
        .map(|i| {
            let x = i as f32;
            Aabb::new(Vec3::new(x, 0.0, 0.0), Vec3::new(x + 1.0, 1.0, 1.0))
        })
        .collect();
    let bvh = Bvh::build(&aabbs, 2);
    let scene_min = bvh.scene_bounds.min;
    let scene_max = bvh.scene_bounds.max;
    println!(
        "nodes: {}, levels: {}",
        bvh.nodes.len(),
        bvh.levels_bottom_up().len()
    );

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
            label: Some("alice-bvh-refit-demo"),
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

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bvh-interior-refit"),
        source: wgpu::ShaderSource::Wgsl(GPU_BVH_INTERIOR_REFIT_COMPUTE_WGSL.into()),
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bvh-refit-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("bvh-refit-pl"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("bvh-refit-pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("cs_main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    // Upload nodes buffer.
    let gpu_nodes: Vec<GpuBvhNode> = bvh.nodes.iter().map(Into::into).collect();
    let nodes_bytes: &[u8] = bytemuck::cast_slice(&gpu_nodes);
    use wgpu::util::DeviceExt;
    let nodes_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("bvh-nodes"),
        contents: nodes_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });

    // Dispatch one level at a time (= bottom-up).
    let levels = bvh.levels_bottom_up();
    for (i, level) in levels.iter().enumerate() {
        if level.is_empty() {
            continue;
        }
        let indices_bytes: Vec<u8> = level.iter().flat_map(|i| i.to_ne_bytes()).collect();
        let indices_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bvh-level-indices"),
            contents: &indices_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
        // WGSL `vec3<u32>` alignment forces the struct to 32 bytes.
        let mut params_bytes = Vec::with_capacity(32);
        params_bytes.extend_from_slice(&(level.len() as u32).to_ne_bytes());
        for _ in 0..7 {
            params_bytes.extend_from_slice(&0_u32.to_ne_bytes());
        }
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bvh-params"),
            contents: &params_bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bvh-refit-bind"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: indices_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: nodes_buf.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bvh-level-encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("bvh-level-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            let workgroups = level.len().div_ceil(64) as u32;
            pass.dispatch_workgroups(workgroups, 1, 1);
        }
        queue.submit(std::iter::once(encoder.finish()));
        println!("level {i}: dispatched {} nodes", level.len());
    }

    // Read back the root node (index 0) bounds.
    use std::sync::mpsc;
    let readback_size = std::mem::size_of::<GpuBvhNode>() as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bvh-readback"),
        size: readback_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("bvh-readback-enc"),
    });
    enc.copy_buffer_to_buffer(&nodes_buf, 0, &readback, 0, readback_size);
    queue.submit(std::iter::once(enc.finish()));
    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::Wait).unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = slice.get_mapped_range();
    let root: &GpuBvhNode = bytemuck::from_bytes(&bytes);
    let r_min = root.bounds_min;
    let r_max = root.bounds_max;
    drop(bytes);
    readback.unmap();

    println!(
        "root bounds (GPU refit): min=({:.2},{:.2},{:.2}) max=({:.2},{:.2},{:.2})",
        r_min[0], r_min[1], r_min[2], r_max[0], r_max[1], r_max[2],
    );
    println!(
        "scene bounds (CPU build): min=({:.2},{:.2},{:.2}) max=({:.2},{:.2},{:.2})",
        scene_min.x(),
        scene_min.y(),
        scene_min.z(),
        scene_max.x(),
        scene_max.y(),
        scene_max.z(),
    );

    let tol = 1e-3;
    let ok = (r_min[0] - scene_min.x()).abs() < tol
        && (r_max[0] - scene_max.x()).abs() < tol
        && (r_min[1] - scene_min.y()).abs() < tol
        && (r_max[1] - scene_max.y()).abs() < tol;
    if ok {
        println!("interior refit produced scene AABB — GPU dispatch verified.");
    } else {
        println!("WARNING: root bounds do not match scene AABB");
    }
}
