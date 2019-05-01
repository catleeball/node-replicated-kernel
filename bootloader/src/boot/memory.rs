use uefi::table::boot::{AllocateType, BootServices, MemoryDescriptor, MemoryType};
use uefi::ResultExt;

use crate::alloc::vec::Vec;
use core::mem;

pub fn test(bt: &BootServices) {
    allocate_pages(bt);
    vec_alloc();

    memory_map(bt);
}

fn allocate_pages(bt: &BootServices) {
    let ty = AllocateType::AnyPages;
    let mem_ty = MemoryType::LOADER_DATA;
    let pgs = bt
        .allocate_pages(ty, mem_ty, 1)
        .expect_success("Failed to allocate a page of memory");

    assert_eq!(pgs % 4096, 0, "Page pointer is not page-aligned");

    // Simple page structure to test this code.
    #[repr(C, align(4096))]
    struct Page([u8; 4096]);

    let page: &Page = unsafe { mem::transmute(pgs) };

    let mut buf = page.0;

    // If these don't fail then we properly allocated some memory.
    buf[0] = 0xF0;
    buf[4095] = 0x23;

    // Clean up to avoid memory leaks.
    bt.free_pages(pgs, 1).unwrap();
}

// Simple test to ensure our custom allocator works with the `alloc` crate.
fn vec_alloc() {
    let mut values = vec![-5, 16, 23, 4, 0];

    values.sort();

    assert_eq!(values[..], [-5, 0, 4, 16, 23], "Failed to sort vector");
}

pub fn memory_map(bt: &BootServices) -> uefi::table::boot::MemoryMapKey {
    // Get an estimate of the memory map size.
    let map_sz = bt.memory_map_size();

    // 8 extra descriptors should be enough.
    let buf_sz = map_sz + 8 * mem::size_of::<MemoryDescriptor>();

    // We will use vectors for convencience.
    let mut buffer = Vec::with_capacity(buf_sz);

    unsafe {
        buffer.set_len(buf_sz);
    }

    let (_key, mut desc_iter) = bt
        .memory_map(&mut buffer)
        .expect_success("Failed to retrieve UEFI memory map");

    // Ensured we have at least one entry.
    // Real memory maps usually have dozens of entries.
    assert!(desc_iter.len() > 0, "Memory map is empty");

    // This is pretty much a sanity test to ensure returned memory isn't filled with random values.
    let first_desc = desc_iter.next().unwrap();

    let phys_start = first_desc.phys_start;

    for entry in desc_iter {
        info!(
            "phys addr: {:x} type: {:?} page count: {}",
            entry.phys_start, entry.ty, entry.page_count
        );
    }

    assert_eq!(phys_start, 0, "Memory does not start at address 0");

    return _key;
}
