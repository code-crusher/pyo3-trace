#![cfg(all(any(not(Py_LIMITED_API), Py_3_13), not(RustPython)))]

//! A `GlobalAlloc` implementation backed by the Python interpreter's raw memory allocator.
//!
//! This module is not meant to be used directly by most users. It provides a
//! [`GlobalAlloc`] implementation that routes Rust allocations through the
//! `PyMem_Raw*` family of allocator functions, which are thread-safe, do not
//! require the calling thread to be attached to the Python interpreter, and
//! can be used before and after the interpreter is initialized.
//!
//! The main motivation for this is [free-threaded Python], where allocating
//! through the default system allocator can perform significantly worse than
//! Python's own allocator under contention. Using this allocator is opt-in:
//! it is enabled by declaring a `#[global_allocator]` in an extension module
//! or embedding application.
//!
//! # Usage
//!
//! ```rust,no_run
//! # #[cfg(all(any(not(Py_LIMITED_API), Py_3_13), not(RustPython)))]
//! # mod example {
//! use pyo3::allocator::PyMemAllocator;
//!
//! #[global_allocator]
//! static GLOBAL: PyMemAllocator = PyMemAllocator;
//! # }
//! ```
//!
//! Only one `#[global_allocator]` can be registered per binary (i.e. per
//! process, counting all extension modules loaded into it), so this should
//! generally be a decision made by the final artifact, not by a library
//! crate.
//!
//! # `tracemalloc` integration
//!
//! Since all Rust allocations go through the raw allocator domain, they become
//! visible to `tracemalloc` and can be traced and reported on with the same
//! tooling Python uses for its own allocations.
//!
//! Note, however, that when `tracemalloc` is enabled, the raw allocator hook
//! may call back into the Python runtime on whichever thread performs the
//! allocation. That may be a thread that is not attached to the interpreter
//! (for example a thread blocked in `Python::detach`, or a pure-Rust thread
//! created by the application), and the hook may need to attach the thread to
//! the runtime in order to record the allocation. Code which assumes it will
//! never call into Python on such threads should take care not to trigger
//! allocations while `tracemalloc` is tracing.
//!
//! # Alternatives
//!
//! If you only want an allocator that avoids the system allocator's
//! contention without going through Python's allocator at all, a general
//! purpose drop-in allocator such as [mimalloc](https://crates.io/crates/mimalloc)
//! is also a valid choice; the advantage of `PyMemAllocator` is that it is
//! already available inside the interpreter and requires no additional
//! native code to be linked into the module.
//!
//! # Availability
//!
//! The `PyMem_Raw*` functions are part of the stable ABI since Python 3.13,
//! and of the version-specific API since Python 3.4 (3.5 for `PyMem_RawCalloc`).
//! This module is therefore available on all supported interpreters except
//! RustPython, and on limited API builds only when targeting a minimum Python
//! version of 3.13 or newer. On PyPy the functions are linked under the
//! `PyPyMem_Raw*` names.
//!
//! [free-threaded Python]: https://docs.python.org/3/howto/free-threading-python.html

use crate::ffi;
use core::alloc::{GlobalAlloc, Layout};
use core::ffi::c_void;
use core::ptr;

/// The alignment guaranteed by the Python raw allocator.
///
/// The raw allocator is documented to return 16-byte aligned memory on 64-bit
/// platforms (and 8-byte aligned on 32-bit platforms), matching the alignment
/// provided by the `malloc` family on those platforms.
#[cfg(target_pointer_width = "64")]
const RAW_ALIGN: usize = 16;

/// The alignment guaranteed by the Python raw allocator.
///
/// The raw allocator is documented to return 16-byte aligned memory on 64-bit
/// platforms (and 8-byte aligned on 32-bit platforms), matching the alignment
/// provided by the `malloc` family on those platforms.
#[cfg(not(target_pointer_width = "64"))]
const RAW_ALIGN: usize = 8;

/// The size of the header stored immediately before the user pointer for
/// over-aligned allocations. The header holds the pointer originally returned
/// by the raw allocator, so that it can be recovered at deallocation time.
const HEADER_SIZE: usize = core::mem::size_of::<*mut c_void>();

/// A [`GlobalAlloc`] implementation that forwards all allocations to the
/// `PyMem_Raw*` allocator functions.
///
/// This type is a zero-sized struct with no state. Construct it as a static
/// and register it with `#[global_allocator]`.
///
/// Allocations with alignment up to that of the raw allocator (16 bytes on
/// 64-bit platforms, 8 bytes on 32-bit platforms) are passed through directly.
/// Allocations with larger alignment requirements are satisfied by
/// over-allocating and storing a pointer to the original allocation in a
/// header word immediately before the returned pointer.
///
/// # Examples
///
/// ```rust,no_run
/// # #[cfg(all(any(not(Py_LIMITED_API), Py_3_13), not(RustPython)))]
/// # mod example {
/// use pyo3::allocator::PyMemAllocator;
///
/// #[global_allocator]
/// static GLOBAL: PyMemAllocator = PyMemAllocator;
/// # }
/// ```
#[cfg_attr(
    docsrs,
    doc(cfg(all(any(not(Py_LIMITED_API), Py_3_13), not(RustPython))))
)]
#[derive(Debug, Default, Clone, Copy)]
pub struct PyMemAllocator;

impl PyMemAllocator {
    /// Returns whether an allocation with this layout can be passed directly to
    /// the raw allocator without any extra alignment handling.
    #[inline]
    const fn is_direct(layout: Layout) -> bool {
        layout.align() <= RAW_ALIGN
    }

    /// Allocate `size` bytes with the raw allocator, with no alignment
    /// adjustment. Callers must ensure `size > 0`, as the `GlobalAlloc`
    /// contract guarantees for all allocations.
    ///
    /// # Safety
    ///
    /// If the returned pointer is non-null, it must eventually be passed to
    /// `PyMem_RawFree` exactly once, and the same `size` must not be
    /// reallocated in place by anyone else.
    #[inline]
    unsafe fn raw_alloc(size: usize) -> *mut u8 {
        // SAFETY: `PyMem_RawMalloc` is thread-safe and may be called without
        // an attached interpreter; the returned pointer, if non-null, is valid
        // for `size` bytes and aligned to at least `RAW_ALIGN`.
        unsafe { ffi::PyMem_RawMalloc(size).cast() }
    }

    /// Allocate `size` bytes of zeroed memory with the raw allocator, with no
    /// alignment adjustment.
    ///
    /// # Safety
    ///
    /// Same contract as [`Self::raw_alloc`].
    #[inline]
    unsafe fn raw_alloc_zeroed(size: usize) -> *mut u8 {
        // SAFETY: `PyMem_RawCalloc` is thread-safe and zero-initializes the
        // returned memory; the returned pointer, if non-null, is valid for
        // `size` bytes and aligned to at least `RAW_ALIGN`.
        unsafe { ffi::PyMem_RawCalloc(size, 1).cast() }
    }

    /// Free a pointer returned by `raw_alloc` (or by `PyMem_RawRealloc` for a
    /// direct allocation).
    ///
    /// # Safety
    ///
    /// `ptr` must either be null, or a pointer previously returned by
    /// `PyMem_RawMalloc`/`PyMem_RawCalloc`/`PyMem_RawRealloc` that has not
    /// been freed since.
    #[inline]
    unsafe fn raw_free(ptr: *mut u8) {
        // SAFETY: forwarded from the caller's contract.
        unsafe { ffi::PyMem_RawFree(ptr.cast()) }
    }

    /// Allocate `size` bytes with the requested alignment, by over-allocating
    /// through the raw allocator and handing out an aligned interior pointer.
    ///
    /// The word immediately before the returned pointer holds the pointer
    /// originally returned by the raw allocator, which is used to recover it
    /// for deallocation.
    ///
    /// # Safety
    ///
    /// `align` must be a power of two strictly greater than `RAW_ALIGN`, and
    /// `size` must be non-zero.
    unsafe fn over_aligned_alloc(size: usize, align: usize) -> *mut u8 {
        debug_assert!(align.is_power_of_two());
        debug_assert!(align > RAW_ALIGN);
        debug_assert!(size > 0);

        // Layout of the raw allocation:
        //
        //   base                          aligned-HEADER_SIZE        aligned               aligned+size
        //   | [ padding up to align-1 ][ header (HEADER_SIZE) ][ size bytes of user data ] |
        //
        // `aligned` is a multiple of `align`, and `aligned >= base + HEADER_SIZE`.
        let extra = align - 1 + HEADER_SIZE;
        let Some(total) = size.checked_add(extra) else {
            return ptr::null_mut();
        };

        // SAFETY: `total` is non-zero (it is at least `align + HEADER_SIZE + 1`),
        // and the returned pointer is checked for null before being used.
        let base = unsafe { Self::raw_alloc(total) };
        if base.is_null() {
            return ptr::null_mut();
        }

        let base_addr = base as usize;
        // Round `base + HEADER_SIZE` up to a multiple of `align`. Both
        // additions cannot overflow because `total >= align + HEADER_SIZE`
        // and `size >= 1`, so `base_addr + total - 1` is a valid address in
        // the allocation.
        let Some(header_end) = base_addr.checked_add(HEADER_SIZE) else {
            // SAFETY: `base` was just allocated and has not been handed out.
            unsafe { Self::raw_free(base) };
            return ptr::null_mut();
        };
        let aligned_addr = header_end.checked_add(align - 1).map(|a| a & !(align - 1));
        let Some(aligned_addr) = aligned_addr else {
            // SAFETY: `base` was just allocated and has not been handed out.
            unsafe { Self::raw_free(base) };
            return ptr::null_mut();
        };

        let user_ptr = aligned_addr as *mut u8;
        let header = user_ptr.wrapping_sub(HEADER_SIZE).cast::<*mut u8>();
        // SAFETY: `header` lies within the raw allocation at
        // `[base, base + total)` because `aligned_addr >= base + HEADER_SIZE`
        // and `header` has room for one pointer. Writing a `*mut u8` to an
        // unaligned address is fine because we use `write_unaligned`.
        unsafe { header.write_unaligned(base) };

        user_ptr
    }

    /// Allocate over-aligned zeroed memory, with the same layout discipline as
    /// [`Self::over_aligned_alloc`].
    #[inline]
    unsafe fn over_aligned_alloc_zeroed(size: usize, align: usize) -> *mut u8 {
        // SAFETY: forwarded from caller; same requirements.
        let ptr = unsafe { Self::over_aligned_alloc(size, align) };
        if !ptr.is_null() {
            // SAFETY: `ptr` is valid for `size` bytes, which is exactly what
            // `Layout` requested.
            unsafe { ptr.write_bytes(0, size) };
        }
        ptr
    }

    /// Recover the raw pointer stored in the header word before `ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` must have been returned by `over_aligned_alloc` (or
    /// `over_aligned_alloc_zeroed`) and not yet freed.
    #[inline]
    unsafe fn over_aligned_raw_ptr(ptr: *mut u8) -> *mut u8 {
        // SAFETY: `ptr` was returned by `over_aligned_alloc`, which stored the
        // original raw pointer in the word immediately before `ptr`.
        unsafe { ptr.sub(HEADER_SIZE).cast::<*mut u8>().read_unaligned() }
    }

    /// Deallocate an over-aligned allocation returned by `over_aligned_alloc`.
    ///
    /// # Safety
    ///
    /// `ptr` must have been returned by `over_aligned_alloc` and must not have
    /// been freed since.
    #[inline]
    unsafe fn over_aligned_free(ptr: *mut u8) {
        // SAFETY: forwarded from caller.
        let raw = unsafe { Self::over_aligned_raw_ptr(ptr) };
        // SAFETY: `raw` was allocated by `raw_alloc` and has not been freed.
        unsafe { Self::raw_free(raw) }
    }
}

// SAFETY: `PyMemAllocator` holds no state, and the `PyMem_Raw*` functions are
// documented as thread-safe, so allocator handles can be shared and moved
// across threads freely.
unsafe impl Send for PyMemAllocator {}

// SAFETY: see above.
unsafe impl Sync for PyMemAllocator {}

// SAFETY: the `GlobalAlloc` contract is upheld as follows:
//
// * `alloc`/`alloc_zeroed` return null on failure and never invoke undefined
//   behavior otherwise. Returned pointers are valid for `layout.size()` bytes
//   and aligned to `layout.align()`: for `align <= RAW_ALIGN` the raw
//   allocator provides the alignment; otherwise the allocation is satisfied by
//   `over_aligned_alloc`, which returns a pointer aligned to `align`.
// * `dealloc` is only called (by callers upholding the contract) with pointers
//   previously returned by this allocator with the same layout, and dispatches
//   to the matching deallocation routine based on the same alignment test.
// * `realloc` preserves the alignment of the original layout, preserves the
//   contents of the allocation up to `min(old_size, new_size)`, and leaves the
//   original allocation untouched on failure.
// * No unwinding occurs out of any of these functions.
unsafe impl GlobalAlloc for PyMemAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if Self::is_direct(layout) {
            // SAFETY: `layout.size() > 0` is guaranteed by the `GlobalAlloc`
            // contract.
            unsafe { Self::raw_alloc(layout.size()) }
        } else {
            // SAFETY: `layout.size() > 0` per the `GlobalAlloc` contract and
            // `layout.align() > RAW_ALIGN` by the branch condition.
            unsafe { Self::over_aligned_alloc(layout.size(), layout.align()) }
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if Self::is_direct(layout) {
            // SAFETY: `layout.size() > 0` is guaranteed by the `GlobalAlloc`
            // contract.
            unsafe { Self::raw_alloc_zeroed(layout.size()) }
        } else {
            // SAFETY: `layout.size() > 0` per the `GlobalAlloc` contract and
            // `layout.align() > RAW_ALIGN` by the branch condition.
            unsafe { Self::over_aligned_alloc_zeroed(layout.size(), layout.align()) }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if Self::is_direct(layout) {
            // SAFETY: `ptr` was returned by `raw_alloc`/`raw_alloc_zeroed`
            // (possibly via `realloc`), hasn't been freed yet, and the layout
            // matches the alignment class it was allocated in.
            unsafe { Self::raw_free(ptr) }
        } else {
            // SAFETY: `ptr` was returned by `over_aligned_alloc` (possibly via
            // `realloc`), hasn't been freed yet, and the layout matches the
            // alignment class it was allocated in.
            unsafe { Self::over_aligned_free(ptr) }
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if Self::is_direct(layout) {
            // `PyMem_RawRealloc` preserves the contents up to the smaller of
            // the old and new sizes, returns null on failure and leaves the
            // original allocation untouched in that case, and never moves the
            // allocation unless required. The alignment requirement is not
            // increased (it is `layout.align() <= RAW_ALIGN` before and after,
            // since the contract keeps `layout.align()`).
            //
            // SAFETY: `ptr` was returned by this allocator with the same
            // alignment class, and `new_size > 0` per the `GlobalAlloc`
            // contract.
            unsafe { ffi::PyMem_RawRealloc(ptr.cast(), new_size).cast() }
        } else {
            // Over-aligned allocations can't use `PyMem_RawRealloc`, because it
            // only guarantees `RAW_ALIGN` alignment. Do an alloc-copy-dealloc
            // dance instead, preserving the alignment.
            let new_layout = match Layout::from_size_align(new_size, layout.align()) {
                Ok(new_layout) => new_layout,
                Err(_) => return ptr::null_mut(),
            };
            // SAFETY: `new_layout.size() == new_size > 0` and
            // `new_layout.align() > RAW_ALIGN`.
            let new_ptr = unsafe { self.alloc(new_layout) };
            if new_ptr.is_null() {
                return ptr::null_mut();
            }
            // SAFETY: `ptr` is valid for `layout.size()` bytes, `new_ptr` is
            // valid for `new_size` bytes, and the two allocations do not
            // overlap.
            unsafe {
                ptr::copy_nonoverlapping(ptr, new_ptr, core::cmp::min(layout.size(), new_size));
            }
            // SAFETY: `ptr` was allocated with `layout` and has not been freed
            // yet.
            unsafe { self.dealloc(ptr, layout) };
            new_ptr
        }
    }
}
