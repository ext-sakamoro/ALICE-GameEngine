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
