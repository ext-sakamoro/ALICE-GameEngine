//! Sparse virtual shadow map data structures (scaffold).
//!
//! UE5-style virtual shadow maps subdivide a single huge logical
//! shadow texture into fixed-size pages (typically 128×128 texels)
//! and only allocate physical pages where the camera actually
//! samples. The big wins:
//!
//! - Constant-cost shadow updates regardless of light count, because
//!   only **dirty** pages re-render each frame.
//! - High effective resolution (= 16k × 16k+) without the VRAM cost
//!   of a dense atlas.
//!
//! This module ships the CPU-side **data model** + page allocator
//! so application code can author dirty-rect masks and reason about
//! the page residency table. The GPU compute pipeline that actually
//! renders into the pages lands in a follow-up PR; the data layout
//! here is the input/output format that pipeline will consume.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VirtualShadowConfig {
    /// Side length of one physical page in texels.
    pub page_size: u32,
    /// Side length of the logical virtual texture in pages.
    pub virtual_pages: u32,
    /// Total physical pages backing the pool. The allocator returns
    /// `None` when this limit is hit.
    pub physical_pages: u32,
}

impl Default for VirtualShadowConfig {
    fn default() -> Self {
        Self {
            page_size: 128,
            virtual_pages: 128, // 128 × 128 = 16,384 px logical
            physical_pages: 4096,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VirtualPageId {
    pub x: u32,
    pub y: u32,
    pub mip: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalPageHandle {
    pub index: u32,
}

#[derive(Debug)]
pub struct VirtualShadowMap {
    pub config: VirtualShadowConfig,
    page_table: HashMap<VirtualPageId, PhysicalPageHandle>,
    free_list: Vec<u32>,
    next_free: u32,
}

impl VirtualShadowMap {
    #[must_use]
    pub fn new(config: VirtualShadowConfig) -> Self {
        Self {
            config,
            page_table: HashMap::new(),
            free_list: Vec::new(),
            next_free: 0,
        }
    }

    /// Allocate (or look up) a physical page backing `virtual_page`.
    /// Returns `None` when the physical pool is exhausted.
    pub fn allocate(&mut self, virtual_page: VirtualPageId) -> Option<PhysicalPageHandle> {
        if let Some(handle) = self.page_table.get(&virtual_page) {
            return Some(*handle);
        }
        let index = if let Some(free) = self.free_list.pop() {
            free
        } else if self.next_free < self.config.physical_pages {
            let i = self.next_free;
            self.next_free += 1;
            i
        } else {
            return None;
        };
        let handle = PhysicalPageHandle { index };
        self.page_table.insert(virtual_page, handle);
        Some(handle)
    }

    /// Release the physical page backing a virtual id, returning it
    /// to the free list. No-op when the page is unmapped.
    pub fn release(&mut self, virtual_page: &VirtualPageId) {
        if let Some(handle) = self.page_table.remove(virtual_page) {
            self.free_list.push(handle.index);
        }
    }

    #[must_use]
    pub fn resident_pages(&self) -> usize {
        self.page_table.len()
    }

    #[must_use]
    pub fn free_pages(&self) -> usize {
        (self.config.physical_pages - self.next_free) as usize + self.free_list.len()
    }

    #[must_use]
    pub fn lookup(&self, virtual_page: VirtualPageId) -> Option<PhysicalPageHandle> {
        self.page_table.get(&virtual_page).copied()
    }
}

// ---------------------------------------------------------------------------
// GPU resources
// ---------------------------------------------------------------------------

/// GPU side of a virtual shadow map: a single 2D atlas texture sized
/// `atlas_pages_per_side² × page_size` and shared by every probe /
/// light. The CPU-side [`VirtualShadowMap`] owns the page table; this
/// struct owns the `wgpu::Texture` + view used for shadow sampling.
#[cfg(feature = "gpu")]
pub struct VirtualShadowGpu {
    pub atlas_texture: wgpu::Texture,
    pub atlas_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub atlas_pages_per_side: u32,
    pub page_size: u32,
}

#[cfg(feature = "gpu")]
impl VirtualShadowGpu {
    /// Allocate the atlas. `atlas_pages_per_side` × `atlas_pages_per_side`
    /// physical pages, each `page_size × page_size` texels.
    #[must_use]
    pub fn new(device: &wgpu::Device, atlas_pages_per_side: u32, page_size: u32) -> Self {
        let edge = atlas_pages_per_side * page_size;
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("alice-virtual-shadow-atlas"),
            size: wgpu::Extent3d {
                width: edge,
                height: edge,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("alice-virtual-shadow-atlas-view"),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("alice-virtual-shadow-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        Self {
            atlas_texture,
            atlas_view,
            sampler,
            atlas_pages_per_side,
            page_size,
        }
    }

    /// Translate a [`PhysicalPageHandle`] into the (u, v) offset of
    /// the page's top-left corner inside the atlas, in normalised
    /// `[0, 1]` UVs.
    #[must_use]
    pub fn page_uv_offset(&self, handle: PhysicalPageHandle) -> (f32, f32) {
        let per_side = self.atlas_pages_per_side.max(1);
        let x = handle.index % per_side;
        let y = handle.index / per_side;
        let inv = (per_side as f32).recip();
        (x as f32 * inv, y as f32 * inv)
    }

    /// Get a one-page `TextureView` (= `array_layer_count: 1`, but
    /// using viewport offsets) so the shadow caster pass can target
    /// exactly one page of the atlas. The returned descriptor sets
    /// the viewport via `set_viewport` on the render pass; the texel
    /// rect is `(x_offset, y_offset, page_size, page_size)`.
    #[must_use]
    pub fn page_viewport(&self, handle: PhysicalPageHandle) -> (f32, f32, f32, f32) {
        let per_side = self.atlas_pages_per_side.max(1);
        let x = (handle.index % per_side) * self.page_size;
        let y = (handle.index / per_side) * self.page_size;
        (
            x as f32,
            y as f32,
            self.page_size as f32,
            self.page_size as f32,
        )
    }

    /// One-page depth render: opens a depth-only render pass on the
    /// atlas restricted to `handle`'s viewport, calls `draw_callback`
    /// with the pass (= application records draw calls there), then
    /// submits.
    ///
    /// Combined with [`Self::page_viewport`] / [`Self::page_uv_offset`]
    /// this is the smallest driver an engine needs to fill exactly
    /// the dirty pages of a virtual shadow map; everything else is
    /// regular render-pass plumbing.
    pub fn render_caster_to_page<F>(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        handle: PhysicalPageHandle,
        clear_depth: f32,
        draw_callback: F,
    ) where
        F: FnOnce(&mut wgpu::RenderPass<'_>),
    {
        let (vx, vy, vw, vh) = self.page_viewport(handle);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("alice-virtual-shadow-page-encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("alice-virtual-shadow-page-pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.atlas_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_depth),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_viewport(vx, vy, vw, vh, 0.0, 1.0);
            draw_callback(&mut pass);
        }
        queue.submit(std::iter::once(encoder.finish()));
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn page(x: u32, y: u32) -> VirtualPageId {
        VirtualPageId { x, y, mip: 0 }
    }

    #[test]
    fn config_default_128x128_pool() {
        let c = VirtualShadowConfig::default();
        assert_eq!(c.page_size, 128);
        assert_eq!(c.virtual_pages, 128);
        assert_eq!(c.physical_pages, 4096);
    }

    #[test]
    fn allocate_returns_sequential_indices() {
        let mut vsm = VirtualShadowMap::new(VirtualShadowConfig::default());
        let a = vsm.allocate(page(0, 0)).unwrap();
        let b = vsm.allocate(page(1, 0)).unwrap();
        let c = vsm.allocate(page(2, 0)).unwrap();
        assert_eq!(a.index, 0);
        assert_eq!(b.index, 1);
        assert_eq!(c.index, 2);
        assert_eq!(vsm.resident_pages(), 3);
    }

    #[test]
    fn allocate_idempotent_for_same_virtual_page() {
        let mut vsm = VirtualShadowMap::new(VirtualShadowConfig::default());
        let a = vsm.allocate(page(5, 7)).unwrap();
        let b = vsm.allocate(page(5, 7)).unwrap();
        assert_eq!(a, b);
        assert_eq!(vsm.resident_pages(), 1);
    }

    #[test]
    fn release_returns_index_to_free_list() {
        let mut vsm = VirtualShadowMap::new(VirtualShadowConfig::default());
        let _ = vsm.allocate(page(0, 0));
        let h = vsm.allocate(page(1, 0)).unwrap();
        vsm.release(&page(0, 0));
        // Next allocate should reuse the released index 0.
        let recycled = vsm.allocate(page(2, 0)).unwrap();
        assert_eq!(recycled.index, 0);
        let _ = h;
    }

    #[test]
    fn allocate_exhausts_pool() {
        let mut vsm = VirtualShadowMap::new(VirtualShadowConfig {
            physical_pages: 3,
            ..VirtualShadowConfig::default()
        });
        assert!(vsm.allocate(page(0, 0)).is_some());
        assert!(vsm.allocate(page(1, 0)).is_some());
        assert!(vsm.allocate(page(2, 0)).is_some());
        assert!(
            vsm.allocate(page(3, 0)).is_none(),
            "pool should be exhausted"
        );
    }

    #[test]
    fn lookup_returns_handle_when_present() {
        let mut vsm = VirtualShadowMap::new(VirtualShadowConfig::default());
        let h = vsm.allocate(page(2, 3)).unwrap();
        assert_eq!(vsm.lookup(page(2, 3)), Some(h));
        assert_eq!(vsm.lookup(page(99, 99)), None);
    }
}
