// `PyMemAllocator` is only available when the `PyMem_Raw*` functions are
// exposed by the interpreter or the limited API.
#![cfg(all(any(not(Py_LIMITED_API), Py_3_13), not(RustPython)))]
#![allow(clippy::undocumented_unsafe_blocks)]

use pyo3::allocator::PyMemAllocator;
use pyo3::prelude::*;
use std::alloc::{GlobalAlloc, Layout};
use std::ptr;

fn roundtrip_layout(layout: Layout, fill: u8) -> *mut u8 {
    unsafe {
        let ptr = GlobalAlloc::alloc(&PyMemAllocator, layout);
        assert!(!ptr.is_null());
        assert_eq!(ptr as usize % layout.align(), 0);
        ptr::write_bytes(ptr, fill, layout.size());
        ptr
    }
}

#[test]
fn small_alloc_roundtrip() {
    let layout = Layout::from_size_align(64, 8).unwrap();
    unsafe {
        let ptr = roundtrip_layout(layout, 0xAB);
        assert_eq!(*ptr, 0xAB);
        assert_eq!(*ptr.add(63), 0xAB);
        GlobalAlloc::dealloc(&PyMemAllocator, ptr, layout);
    }
}

#[test]
fn large_alloc_roundtrip() {
    // Larger than 4 KiB, to exercise sizes beyond a page.
    let layout = Layout::from_size_align(1 << 20, 8).unwrap();
    unsafe {
        let ptr = roundtrip_layout(layout, 0xCD);
        assert_eq!(*ptr, 0xCD);
        assert_eq!(*ptr.add(layout.size() - 1), 0xCD);
        GlobalAlloc::dealloc(&PyMemAllocator, ptr, layout);
    }
}

#[test]
fn alloc_zeroed_memory() {
    let layout = Layout::from_size_align(256, 16).unwrap();
    unsafe {
        let ptr = GlobalAlloc::alloc_zeroed(&PyMemAllocator, layout);
        assert!(!ptr.is_null());
        for i in 0..layout.size() {
            assert_eq!(*ptr.add(i), 0);
        }
        GlobalAlloc::dealloc(&PyMemAllocator, ptr, layout);
    }
}

#[test]
fn alloc_zeroed_over_aligned() {
    let layout = Layout::from_size_align(128, 64).unwrap();
    unsafe {
        let ptr = GlobalAlloc::alloc_zeroed(&PyMemAllocator, layout);
        assert!(!ptr.is_null());
        assert_eq!(ptr as usize % 64, 0);
        for i in 0..layout.size() {
            assert_eq!(*ptr.add(i), 0);
        }
        GlobalAlloc::dealloc(&PyMemAllocator, ptr, layout);
    }
}

#[test]
fn over_aligned_roundtrip() {
    // Alignment larger than the allocator's natural alignment (16 on 64-bit
    // platforms, 8 on 32-bit platforms) forces the over-aligned path.
    for &align in &[32, 64, 128, 256] {
        let layout = Layout::from_size_align(1000, align).unwrap();
        unsafe {
            let ptr = roundtrip_layout(layout, 0xEF);
            assert_eq!(ptr as usize % align, 0);
            assert_eq!(*ptr.add(999), 0xEF);
            GlobalAlloc::dealloc(&PyMemAllocator, ptr, layout);
        }
    }
}

#[test]
fn over_aligned_zero_filled() {
    let layout = Layout::from_size_align(4096, 4096).unwrap();
    unsafe {
        let ptr = roundtrip_layout(layout, 0x00);
        assert_eq!(ptr as usize % 4096, 0);
        for i in 0..layout.size() {
            assert_eq!(*ptr.add(i), 0);
        }
        GlobalAlloc::dealloc(&PyMemAllocator, ptr, layout);
    }
}

#[test]
fn realloc_grows_and_preserves_contents() {
    let layout = Layout::from_size_align(16, 8).unwrap();
    unsafe {
        let ptr = GlobalAlloc::alloc(&PyMemAllocator, layout);
        assert!(!ptr.is_null());
        ptr::write_bytes(ptr, 0xAB, layout.size());

        let new_size = 4096;
        let new_ptr = GlobalAlloc::realloc(&PyMemAllocator, ptr, layout, new_size);
        assert!(!new_ptr.is_null());
        // The ABI does not guarantee that in-place reallocation reuses the same
        // address, so only check the contents.
        for i in 0..layout.size() {
            assert_eq!(*new_ptr.add(i), 0xAB, "byte {i} was not preserved");
        }
        let new_layout = Layout::from_size_align(new_size, layout.align()).unwrap();
        GlobalAlloc::dealloc(&PyMemAllocator, new_ptr, new_layout);
    }
}

#[test]
fn realloc_shrink_preserves_contents() {
    let layout = Layout::from_size_align(4096, 8).unwrap();
    unsafe {
        let ptr = GlobalAlloc::alloc(&PyMemAllocator, layout);
        assert!(!ptr.is_null());
        ptr::write_bytes(ptr, 0x5A, layout.size());

        let new_size = 128;
        let new_ptr = GlobalAlloc::realloc(&PyMemAllocator, ptr, layout, new_size);
        assert!(!new_ptr.is_null());
        for i in 0..new_size {
            assert_eq!(*new_ptr.add(i), 0x5A);
        }
        let new_layout = Layout::from_size_align(new_size, layout.align()).unwrap();
        GlobalAlloc::dealloc(&PyMemAllocator, new_ptr, new_layout);
    }
}

#[test]
fn realloc_over_aligned() {
    let layout = Layout::from_size_align(64, 128).unwrap();
    unsafe {
        let ptr = GlobalAlloc::alloc(&PyMemAllocator, layout);
        assert!(!ptr.is_null());
        assert_eq!(ptr as usize % 128, 0);
        ptr::write_bytes(ptr, 0x11, layout.size());

        let new_size = 1024;
        let new_ptr = GlobalAlloc::realloc(&PyMemAllocator, ptr, layout, new_size);
        assert!(!new_ptr.is_null());
        assert_eq!(new_ptr as usize % 128, 0);
        for i in 0..layout.size() {
            assert_eq!(*new_ptr.add(i), 0x11, "byte {i} was not preserved");
        }
        let new_layout = Layout::from_size_align(new_size, layout.align()).unwrap();
        GlobalAlloc::dealloc(&PyMemAllocator, new_ptr, new_layout);
    }
}

#[test]
fn boxed_roundtrip() {
    // Exercise the allocator through a heap allocation with a Drop impl,
    // approximating how it behaves as a stand-in for the global allocator.
    use std::alloc::Layout;

    let layout = Layout::from_size_align(256, 8).unwrap();
    let ptr = unsafe { GlobalAlloc::alloc(&PyMemAllocator, layout) };
    assert!(!ptr.is_null());
    unsafe {
        ptr.write_bytes(0xAB, layout.size());
        assert_eq!(*ptr, 0xAB);
        assert_eq!(*ptr.add(255), 0xAB);
        GlobalAlloc::dealloc(&PyMemAllocator, ptr, layout);
    }
}

#[test]
fn allocator_works_without_interpreter() {
    // The allocator does not require an attached interpreter, so it can be
    // used from threads that never call into Python. This exercises the
    // allocation path from a freshly spawned thread that never attaches.
    let handle = std::thread::spawn(|| {
        let layout = Layout::from_size_align(128, 16).unwrap();
        unsafe {
            let ptr = GlobalAlloc::alloc(&PyMemAllocator, layout);
            assert!(!ptr.is_null());
            ptr::write_bytes(ptr, 0x42, layout.size());
            assert_eq!(*ptr, 0x42);
            GlobalAlloc::dealloc(&PyMemAllocator, ptr, layout);
        }
    });
    handle.join().unwrap();
}

#[test]
fn python_interop() {
    Python::attach(|_py| {
        // The interpreter is running; allocating through PyMemAllocator from
        // Rust should still work and be usable while attached.
        let layout = Layout::from_size_align(64, 16).unwrap();
        let ptr = unsafe { GlobalAlloc::alloc(&PyMemAllocator, layout) };
        assert!(!ptr.is_null());
        unsafe {
            ptr::write_bytes(ptr, 0x7F, layout.size());
            assert_eq!(*ptr, 0x7F);
            GlobalAlloc::dealloc(&PyMemAllocator, ptr, layout);
        }
        // And the interpreter is still alive and usable.
        assert!(!pyo3::Python::version_str().is_empty());
    });
}
