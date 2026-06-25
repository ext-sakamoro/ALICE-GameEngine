//! Cubemap sky GPU render demo — drives
//! [`CubemapCaptureTargets::render_sky_to_faces`] on a real GPU,
//! reads back the centre texel of every face, and prints them.
//!
//! ```bash
//! cargo run --example cubemap_sky_demo --features gpu
//! ```

use alice_game_engine::env_probe::CubemapCaptureTargets;
use alice_game_engine::math::Vec3;
use alice_game_engine::sky::AtmosphereParams;

fn main() {
    println!("=== Cubemap Sky GPU Demo ===");

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
            label: Some("alice-cubemap-sky-demo"),
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

    let resolution = 64_u32;
    let targets = CubemapCaptureTargets::new(&device, Vec3::ZERO, resolution, 0.1, 100.0);
    let atmosphere = AtmosphereParams {
        sun_direction: Vec3::new(0.3, 0.8, -0.5),
        day_phase: 1.0,
        fog_density: 0.0,
        cloud_cover: 0.0,
    };

    let bytes = targets.render_sky_to_faces(&device, &queue, &atmosphere);
    println!("submitted {bytes} bytes of uniform data across 6 faces");

    // Readback every face's centre texel.
    let texel_size = 8_u32; // Rgba16Float = 4 channels × 2 bytes
    let bytes_per_row = resolution * texel_size;
    let face_size = (bytes_per_row * resolution) as u64;
    let total_size = face_size * 6;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cubemap-readback"),
        size: total_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("cubemap-readback-enc"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &targets.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(resolution),
            },
        },
        wgpu::Extent3d {
            width: resolution,
            height: resolution,
            depth_or_array_layers: 6,
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
    let data = slice.get_mapped_range();

    let face_names = ["+X", "-X", "+Y", "-Y", "+Z", "-Z"];
    let mut any_nonzero = false;
    for face in 0..6 {
        let centre_offset = (face as u64) * face_size
            + ((resolution / 2) as u64) * (bytes_per_row as u64)
            + ((resolution / 2) as u64) * (texel_size as u64);
        let raw = &data[centre_offset as usize..centre_offset as usize + 8];
        // Decode Rgba16Float (= half-precision floats).
        let r = half_to_f32(u16::from_le_bytes([raw[0], raw[1]]));
        let g = half_to_f32(u16::from_le_bytes([raw[2], raw[3]]));
        let b = half_to_f32(u16::from_le_bytes([raw[4], raw[5]]));
        if r > 0.0 || g > 0.0 || b > 0.0 {
            any_nonzero = true;
        }
        println!(
            "  face {} centre: ({:.3}, {:.3}, {:.3})",
            face_names[face], r, g, b
        );
    }
    drop(data);
    readback.unmap();

    if any_nonzero {
        println!("sky shader populated the cubemap — GPU dispatch verified.");
    } else {
        println!("WARNING: every face is black; the sky shader did not write");
    }
}

fn half_to_f32(h: u16) -> f32 {
    let sign = (h >> 15) & 0x1;
    let exp = (h >> 10) & 0x1f;
    let mant = h & 0x3ff;
    let f = match exp {
        0 => {
            if mant == 0 {
                0.0
            } else {
                // Subnormal — rare in practice for sky values.
                (mant as f32) * (2.0_f32).powi(-24)
            }
        }
        31 => f32::INFINITY,
        _ => (mant as f32 + 1024.0) * (2.0_f32).powi((exp as i32) - 25),
    };
    if sign == 1 {
        -f
    } else {
        f
    }
}
