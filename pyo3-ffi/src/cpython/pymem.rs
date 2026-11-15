use core::ffi::c_void;
use libc::size_t;

// The `PyMem_Raw*` functions are declared in `pymem_raw` instead, so that they can be
// exposed in limited API builds where the rest of this module is unavailable.
// skipped _PyMem_GetCurrentAllocatorName
// skipped _PyMem_RawStrdup
// skipped _PyMem_Strdup
// skipped _PyMem_RawWcsdup

#[repr(C)]
#[derive(Copy, Clone)]
pub enum PyMemAllocatorDomain {
    PYMEM_DOMAIN_RAW,
    PYMEM_DOMAIN_MEM,
    PYMEM_DOMAIN_OBJ,
}

// skipped PyMemAllocatorName
#[cfg(not(any(PyPy, GraalPy)))]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct PyMemAllocatorEx {
    pub ctx: *mut c_void,
    pub malloc: Option<extern "C" fn(ctx: *mut c_void, size: size_t) -> *mut c_void>,
    pub calloc:
        Option<extern "C" fn(ctx: *mut c_void, nelem: size_t, elsize: size_t) -> *mut c_void>,
    pub realloc:
        Option<extern "C" fn(ctx: *mut c_void, ptr: *mut c_void, new_size: size_t) -> *mut c_void>,
    pub free: Option<extern "C" fn(ctx: *mut c_void, ptr: *mut c_void)>,
}

extern_libpython! {
    #[cfg(not(any(PyPy, GraalPy)))]
    pub fn PyMem_GetAllocator(domain: PyMemAllocatorDomain, allocator: *mut PyMemAllocatorEx);
    #[cfg(not(any(PyPy, GraalPy)))]
    pub fn PyMem_SetAllocator(domain: PyMemAllocatorDomain, allocator: *mut PyMemAllocatorEx);
    #[cfg(not(any(PyPy, GraalPy)))]
    pub fn PyMem_SetupDebugHooks();
}
