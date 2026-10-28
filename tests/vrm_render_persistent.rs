//! The toon renderer keeps its GPU objects from one image to the next.
//!
//! oracle (the contract, counted by hand; the counts are `ToonRenderer::gpu_created`): a part
//! needs a vertex, an index and a parameter buffer and a bind group; a drawing needs a depth
//! target, and at 4 samples a multisampled colour target; a readback (`draw_parts`) needs a target
//! and a readback buffer; a texture is one upload. After the first image of a scene:
//! - drawing the same parts again, with moved vertices, another view or light, makes nothing
//!   (where GPU memory is freed only by a garbage collector, as in a browser, an object a frame
//!   grows without bound);
//! - `resize` makes again only the size-bound targets: depth + multisampled colour (2 at 4
//!   samples), and for a readback its target and buffer;
//! - a part whose mesh grows makes again only its own vertex and index buffers; a part whose
//!   texture changes uploads that texture and makes only its own bind group;
//! - an unkeyed texture whose bytes are unchanged is not uploaded again;
//! - each image is byte for byte the image a fresh renderer draws of the same parts (a kept object
//!   never shows a previous frame's contents).

#[cfg(feature = "gpu")]
mod gpu {
    use alice_game_engine::vrm_render::{
        AlphaMode, GpuCreated, Outline, OutlineMode, Surface, ToonParams, ToonPart, ToonRenderer,
        ToonTexture, UvTransform,
    };

    const CLEAR: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
    const UP: [[f32; 3]; 4] = [[0.0, 0.0, 1.0]; 4];
    const UVS: [[f32; 2]; 4] = [[0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]];
    const TRI: [u32; 3] = [0, 1, 2];
    const QUAD: [u32; 6] = [0, 1, 2, 2, 1, 3];
    const MAGENTA: [u8; 4] = [255, 0, 128, 255];
    const CYAN: [u8; 4] = [0, 200, 255, 255];
    const CHECK: [u8; 16] = [
        255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255, 255, 0, 0, 255,
    ];

    /// The view-projection taking world (x, y) to clip (x - 0.5 - dx, y - 0.5)
    fn view_proj(dx: f32) -> [f32; 16] {
        [
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.5,
            0.0,
            -0.5 - dx,
            -0.5,
            0.5,
            1.0,
        ]
    }

    fn params(dx: f32, light: [f32; 3]) -> ToonParams {
        ToonParams {
            view_proj: view_proj(dx),
            base_color: [1.0; 4],
            shade_color: [0.6, 0.6, 0.7, 1.0],
            light_dir: light,
            shading_shift: 0.0,
            shading_toony: 0.9,
            clear: [0.5; 4],
        }
    }

    fn tex(rgba: &[u8], w: u32, key: Option<u64>) -> ToonTexture<'_> {
        ToonTexture {
            width: w,
            height: (rgba.len() / 4) as u32 / w,
            rgba,
            key,
        }
    }

    fn part<'a>(
        pos: &'a [[f32; 3]],
        idx: &'a [u32],
        p: ToonParams,
        base: Option<ToonTexture<'a>>,
    ) -> ToonPart<'a> {
        ToonPart {
            positions: pos,
            normals: &UP[..pos.len()],
            indices: idx,
            params: p,
            paint: None,
            outline: Some(Outline {
                mode: OutlineMode::World,
                width: 0.01,
                color: [0.2, 0.1, 0.1],
                lighting_mix: 0.5,
            }),
            surface: base.map(|b| Surface {
                uvs: &UVS[..pos.len()],
                base: Some(b),
                shade: None,
                transform: UvTransform::IDENTITY,
                alpha: AlphaMode::Opaque,
            }),
        }
    }

    /// Triangle (first 3) or quad, its lower-left corner at (x, y), side 0.6, depth z
    fn shape(x: f32, y: f32, z: f32) -> [[f32; 3]; 4] {
        [
            [x, y, z],
            [x + 0.6, y, z],
            [x, y + 0.6, z],
            [x + 0.6, y + 0.6, z],
        ]
    }

    fn diff(a: GpuCreated, b: GpuCreated) -> GpuCreated {
        GpuCreated {
            buffers: b.buffers - a.buffers,
            textures: b.textures - a.textures,
            bind_groups: b.bind_groups - a.bind_groups,
        }
    }

    const NOTHING: GpuCreated = GpuCreated {
        buffers: 0,
        textures: 0,
        bind_groups: 0,
    };

    /// What a fresh renderer of the same size and samples draws of the parts
    fn fresh(w: u32, h: u32, samples: u32, parts: &[ToonPart<'_>]) -> Vec<u8> {
        let mut r = ToonRenderer::new_headless(w, h).expect("a GPU");
        r.set_samples(samples).unwrap();
        r.draw_parts(parts, CLEAR).unwrap()
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_frame_after_the_first_makes_no_gpu_object() {
        for samples in [1, 4] {
            let mut r = ToonRenderer::new_headless(64, 64).expect("a GPU");
            r.set_samples(samples).unwrap();
            let a = shape(0.1, 0.1, 0.5);
            let b = shape(0.3, 0.3, 0.4);
            for k in 0..8 {
                // the vertices move, the view and the light turn at every frame
                let t = k as f32 * 0.01;
                let a_k: Vec<[f32; 3]> = a.iter().map(|p| [p[0] + t, p[1], p[2]]).collect();
                let b_k: Vec<[f32; 3]> = b.iter().map(|p| [p[0], p[1] + t, p[2]]).collect();
                let light = [t, 0.3, 1.0];
                // a keyed texture, an unkeyed one, and an untextured part (the renderer's white)
                let parts = [
                    part(&a_k, &QUAD, params(t, light), Some(tex(&CHECK, 2, Some(7)))),
                    part(
                        &b_k[..3],
                        &TRI,
                        params(t, light),
                        Some(tex(&MAGENTA, 1, None)),
                    ),
                    part(&b_k, &QUAD, params(t, light), None),
                ];
                let before = r.gpu_created();
                let img = r.draw_parts(&parts, CLEAR).unwrap();
                let made = diff(before, r.gpu_created());
                if k > 0 {
                    assert_eq!(made, NOTHING, "samples {samples}, frame {k}: made {made:?}");
                }
                assert_eq!(
                    img,
                    fresh(64, 64, samples, &parts),
                    "samples {samples}, frame {k}"
                );
            }
        }
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_frame_into_a_view_after_the_first_makes_no_gpu_object() {
        // a caller's view of the renderer's size (a canvas): only the renderer's objects counted
        let instance = wgpu::Instance::default();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("a GPU");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("a device");
        let mut r2 = ToonRenderer::with_device(
            device.clone(),
            queue,
            wgpu::TextureFormat::Rgba8Unorm,
            64,
            64,
        )
        .unwrap();
        r2.set_samples(4).unwrap();
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let a = shape(0.1, 0.1, 0.5);
        for k in 0..8 {
            let t = k as f32 * 0.01;
            let a_k: Vec<[f32; 3]> = a.iter().map(|p| [p[0] + t, p[1], p[2]]).collect();
            let parts = [
                part(
                    &a_k,
                    &QUAD,
                    params(t, [0.0, 0.3, 1.0]),
                    Some(tex(&CHECK, 2, Some(7))),
                ),
                part(&a_k[..3], &TRI, params(t, [0.0, 0.3, 1.0]), None),
            ];
            let before = r2.gpu_created();
            r2.draw_parts_into(&parts, CLEAR, &view).unwrap();
            let made = diff(before, r2.gpu_created());
            if k == 0 {
                // 2 parts x (3 buffers + 1 bind group), the keyed texture, the white, the 2 targets
                assert_eq!(
                    made,
                    GpuCreated {
                        buffers: 6,
                        textures: 4,
                        bind_groups: 2
                    }
                );
            } else {
                assert_eq!(made, NOTHING, "frame {k}: made {made:?}");
            }
        }
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn resize_makes_again_only_the_size_bound_targets() {
        let mut r = ToonRenderer::new_headless(64, 64).expect("a GPU");
        r.set_samples(4).unwrap();
        let a = shape(0.1, 0.1, 0.5);
        let parts = [part(&a, &QUAD, params(0.0, [0.0, 0.3, 1.0]), None)];
        r.draw_parts(&parts, CLEAR).unwrap();
        r.resize(32, 48).unwrap();
        let before = r.gpu_created();
        let img = r.draw_parts(&parts, CLEAR).unwrap();
        // depth + multisampled colour + readback target; the readback buffer
        assert_eq!(
            diff(before, r.gpu_created()),
            GpuCreated {
                buffers: 1,
                textures: 3,
                bind_groups: 0
            }
        );
        assert_eq!(img, fresh(32, 48, 4, &parts));
        let before = r.gpu_created();
        r.draw_parts(&parts, CLEAR).unwrap();
        assert_eq!(diff(before, r.gpu_created()), NOTHING);
        // the same size again is not a change
        r.resize(32, 48).unwrap();
        let before = r.gpu_created();
        r.draw_parts(&parts, CLEAR).unwrap();
        assert_eq!(diff(before, r.gpu_created()), NOTHING);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_changed_part_makes_again_only_its_own_objects() {
        let mut r = ToonRenderer::new_headless(64, 64).expect("a GPU");
        r.set_samples(4).unwrap();
        let a = shape(0.1, 0.1, 0.5);
        let b = shape(0.3, 0.3, 0.4);
        let p = params(0.0, [0.0, 0.3, 1.0]);
        r.draw_parts(
            &[
                part(&a, &QUAD, p, Some(tex(&CHECK, 2, Some(7)))),
                part(&b[..3], &TRI, p, None),
            ],
            CLEAR,
        )
        .unwrap();
        // part 1 grows from a triangle to a quad: its vertex and index buffers, nothing else
        let parts = [
            part(&a, &QUAD, p, Some(tex(&CHECK, 2, Some(7)))),
            part(&b, &QUAD, p, None),
        ];
        let before = r.gpu_created();
        let img = r.draw_parts(&parts, CLEAR).unwrap();
        assert_eq!(
            diff(before, r.gpu_created()),
            GpuCreated {
                buffers: 2,
                textures: 0,
                bind_groups: 0
            }
        );
        assert_eq!(img, fresh(64, 64, 4, &parts));
        // part 0 takes another keyed texture: one upload, its bind group
        let parts = [
            part(&a, &QUAD, p, Some(tex(&CYAN, 1, Some(8)))),
            part(&b, &QUAD, p, None),
        ];
        let before = r.gpu_created();
        let img = r.draw_parts(&parts, CLEAR).unwrap();
        assert_eq!(
            diff(before, r.gpu_created()),
            GpuCreated {
                buffers: 0,
                textures: 1,
                bind_groups: 1
            }
        );
        assert_eq!(img, fresh(64, 64, 4, &parts));
        // back to the first texture: it is still in the keyed cache, only the bind group
        let parts = [
            part(&a, &QUAD, p, Some(tex(&CHECK, 2, Some(7)))),
            part(&b, &QUAD, p, None),
        ];
        let before = r.gpu_created();
        let img = r.draw_parts(&parts, CLEAR).unwrap();
        assert_eq!(
            diff(before, r.gpu_created()),
            GpuCreated {
                buffers: 0,
                textures: 0,
                bind_groups: 1
            }
        );
        assert_eq!(img, fresh(64, 64, 4, &parts));
        // an unkeyed texture whose bytes change: uploaded again, drawn with its own bytes
        let parts = [
            part(&a, &QUAD, p, Some(tex(&MAGENTA, 1, None))),
            part(&b, &QUAD, p, None),
        ];
        r.draw_parts(&parts, CLEAR).unwrap();
        let parts = [
            part(&a, &QUAD, p, Some(tex(&CYAN, 1, None))),
            part(&b, &QUAD, p, None),
        ];
        let before = r.gpu_created();
        let img = r.draw_parts(&parts, CLEAR).unwrap();
        assert_eq!(
            diff(before, r.gpu_created()),
            GpuCreated {
                buffers: 0,
                textures: 1,
                bind_groups: 1
            }
        );
        assert_eq!(img, fresh(64, 64, 4, &parts));
        // fewer parts, then as many again: the dropped slot is made again
        let before = r.gpu_created();
        r.draw_parts(&parts[..1], CLEAR).unwrap();
        assert_eq!(diff(before, r.gpu_created()), NOTHING);
        let before = r.gpu_created();
        let img = r.draw_parts(&parts, CLEAR).unwrap();
        assert_eq!(
            diff(before, r.gpu_created()),
            GpuCreated {
                buffers: 3,
                textures: 0,
                bind_groups: 1
            }
        );
        assert_eq!(img, fresh(64, 64, 4, &parts));
    }
}
