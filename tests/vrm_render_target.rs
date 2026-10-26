//! Drawing on a caller's device and view (a browser canvas, a window surface), resizing, and
//! keyed textures.
//!
//! oracle (worked by hand): the scene of `vrm_surface` (triangle FAR = (0.1, 0.1), (0.9, 0.1),
//! (0.1, 0.9), the view-projection taking world (x, y) to clip (x - 0.5, y - 0.5), pixel (38, 43)
//! of a 64 x 64 image inside FAR, (60, 4) outside), a 1 x 1 texture of one colour at every UV:
//! - `draw_parts_into` a view of the renderer's format gives the bytes `draw_parts` returns; on a
//!   `Bgra8Unorm` renderer the same image with red and blue swapped, and `draw_parts` refuses to
//!   read it back;
//! - after `resize(32, 32)` the image is 32 x 32 and pixel (19, 21) (world (0.719, 0.156), inside FAR
//!   as on the 64 x 64 image) is FAR's colour, (30, 2) the clear colour;
//! - a keyed texture is uploaded once: a later part with the same key and other bytes draws the
//!   first bytes, `forget_textures` makes it draw the new ones, an unkeyed texture draws its own
//!   bytes at every draw.

#[cfg(feature = "gpu")]
mod gpu {
    use alice_game_engine::vrm_render::{
        AlphaMode, RenderError, Surface, ToonParams, ToonPart, ToonRenderer, ToonTexture,
        UvTransform,
    };

    const VIEW_PROJ: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
    ];
    const CLEAR: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
    const UP: [[f32; 3]; 3] = [[0.0, 0.0, 1.0]; 3];
    const FAR: [[f32; 3]; 3] = [[0.1, 0.1, 0.5], [0.9, 0.1, 0.5], [0.1, 0.9, 0.5]];
    const TRI: [u32; 3] = [0, 1, 2];
    const UVS: [[f32; 2]; 3] = [[0.5, 0.5]; 3];

    fn part<'a>(tex: ToonTexture<'a>) -> ToonPart<'a> {
        ToonPart {
            positions: &FAR,
            normals: &UP,
            indices: &TRI,
            params: ToonParams {
                view_proj: VIEW_PROJ,
                base_color: [1.0; 4],
                shade_color: [1.0; 4],
                light_dir: [0.0, 0.0, 1.0],
                shading_shift: 0.0,
                shading_toony: 1.0,
                clear: [0.5; 4],
            },
            paint: None,
            outline: None,
            surface: Some(Surface {
                uvs: &UVS,
                base: Some(tex),
                shade: None,
                transform: UvTransform::IDENTITY,
                alpha: AlphaMode::Opaque,
            }),
        }
    }

    fn one(rgba: &[u8; 4], key: Option<u64>) -> ToonTexture<'_> {
        ToonTexture {
            width: 1,
            height: 1,
            rgba,
            key,
        }
    }

    fn device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::default();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("a GPU");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("a device")
    }

    /// Draws into a fresh texture of `format` and reads it back, rows top first
    fn draw_into(
        r: &ToonRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        parts: &[ToonPart<'_>],
    ) -> Vec<u8> {
        let size = wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        };
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        r.draw_parts_into(parts, CLEAR, &view).unwrap();
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256 * 64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(64),
                },
            },
            size,
        );
        queue.submit(Some(enc.finish()));
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device.poll(wgpu::PollType::Wait).unwrap();
        let data = slice.get_mapped_range().to_vec();
        data
    }

    fn pixel(img: &[u8], width: usize, col: usize, row: usize) -> [u8; 4] {
        let i = (row * width + col) * 4;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    const MAGENTA: [u8; 4] = [255, 0, 128, 255];
    const CYAN: [u8; 4] = [0, 200, 255, 255];

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn drawing_into_a_view_gives_the_read_back_image() {
        let (device, queue) = device();
        let r = ToonRenderer::with_device(
            device.clone(),
            queue.clone(),
            wgpu::TextureFormat::Rgba8Unorm,
            64,
            64,
        )
        .unwrap();
        let parts = [part(one(&MAGENTA, None))];
        let back = r.draw_parts(&parts, CLEAR).unwrap();
        let into = draw_into(&r, &device, &queue, wgpu::TextureFormat::Rgba8Unorm, &parts);
        assert_eq!(pixel(&back, 64, 38, 43), MAGENTA);
        assert_eq!(pixel(&back, 64, 60, 4), [0, 255, 0, 255]);
        assert_eq!(into, back);
        // a Bgra8Unorm renderer: the same picture, red and blue swapped; no readback
        let bgra = ToonRenderer::with_device(
            device.clone(),
            queue.clone(),
            wgpu::TextureFormat::Bgra8Unorm,
            64,
            64,
        )
        .unwrap();
        let swapped = draw_into(
            &bgra,
            &device,
            &queue,
            wgpu::TextureFormat::Bgra8Unorm,
            &parts,
        );
        let unswapped: Vec<u8> = swapped
            .chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
        assert_eq!(unswapped, back);
        assert!(matches!(
            bgra.draw_parts(&parts, CLEAR),
            Err(RenderError::Readback(_))
        ));
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_resized_renderer_draws_the_new_size() {
        let mut r = ToonRenderer::new_headless(64, 64).expect("a GPU");
        let parts = [part(one(&MAGENTA, None))];
        r.resize(32, 32).unwrap();
        let img = r.draw_parts(&parts, CLEAR).unwrap();
        assert_eq!(img.len(), 32 * 32 * 4);
        assert_eq!(pixel(&img, 32, 19, 21), MAGENTA);
        assert_eq!(pixel(&img, 32, 30, 2), [0, 255, 0, 255]);
        r.resize(64, 64).unwrap();
        assert_eq!(r.draw_parts(&parts, CLEAR).unwrap().len(), 64 * 64 * 4);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn sizes_outside_the_range_are_refused() {
        let (device, queue) = device();
        let f = wgpu::TextureFormat::Rgba8Unorm;
        for (w, h) in [(0, 1), (1, 0), (8193, 1), (1, 8193)] {
            assert!(matches!(
                ToonRenderer::with_device(device.clone(), queue.clone(), f, w, h),
                Err(RenderError::BadMesh(_))
            ));
        }
        let mut r = ToonRenderer::with_device(device, queue, f, 8192, 1).unwrap();
        for (w, h) in [(0, 1), (1, 0), (8193, 1), (1, 8193)] {
            assert!(matches!(r.resize(w, h), Err(RenderError::BadMesh(_))));
        }
        r.resize(1, 8192).unwrap();
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_keyed_texture_is_uploaded_once_until_forgotten() {
        let r = ToonRenderer::new_headless(64, 64).expect("a GPU");
        let at = |t: ToonTexture<'_>| pixel(&r.draw_parts(&[part(t)], CLEAR).unwrap(), 64, 38, 43);
        assert_eq!(at(one(&MAGENTA, Some(7))), MAGENTA);
        // the same key: the first upload, whatever the bytes now
        assert_eq!(at(one(&CYAN, Some(7))), MAGENTA);
        // another key, or none: its own bytes
        assert_eq!(at(one(&CYAN, Some(8))), CYAN);
        assert_eq!(at(one(&CYAN, None)), CYAN);
        assert_eq!(at(one(&MAGENTA, None)), MAGENTA);
        r.forget_textures();
        assert_eq!(at(one(&CYAN, Some(7))), CYAN);
    }
}
