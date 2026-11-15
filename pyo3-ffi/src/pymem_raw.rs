//! The raw memory interface
//!
//! Wrappers for the `PyMem_Raw*` family of allocator functions. These are
//! thread-safe and can be used without attaching to the Python interpreter,
//! unlike most of the Python C API.
//!
//! The default raw allocator wraps the system allocator, and can be configured
//! with `PyMem_SetAllocator(PYMEM_DOMAIN_RAW, ...)`. These functions are part of
//! the stable ABI since Python 3.13, and have been available in the
//! version-specific ABI since Python 3.4 (and Python 3.5 for `PyMem_RawCalloc`).
//!
//! Note that on PyPy before 3.12 these are linked under the `PyPyMem_Raw*`
//! names, matching the rest of PyO3's handling of PyPy's allocator exports;
//! newer PyPy versions export the `PyMem_Raw*` symbols directly.

use core::ffi::c_void;
use libc::size_t;

extern_libpython! {
    #[cfg_attr(all(PyPy, not(Py_3_12)), link_name = "PyPyMem_RawMalloc")]
    pub fn PyMem_RawMalloc(size: size_t) -> *mut c_void;
    #[cfg_attr(all(PyPy, not(Py_3_12)), link_name = "PyPyMem_RawCalloc")]
    pub fn PyMem_RawCalloc(nelem: size_t, elsize: size_t) -> *mut c_void;
    #[cfg_attr(all(PyPy, not(Py_3_12)), link_name = "PyPyMem_RawRealloc")]
    pub fn PyMem_RawRealloc(ptr: *mut c_void, new_size: size_t) -> *mut c_void;
    #[cfg_attr(all(PyPy, not(Py_3_12)), link_name = "PyPyMem_RawFree")]
    pub fn PyMem_RawFree(ptr: *mut c_void);
}
