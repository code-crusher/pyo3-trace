use crate::err::PyResult;
use crate::ffi_ptr_ext::FfiPtrExt;
use crate::py_result_ext::PyResultExt;
use crate::{Bound, PyAny, ffi};
#[cfg(RustPython)]
use crate::{
    Py,
    sync::PyOnceLock,
    types::{PyType, PyTypeMethods},
};

/// Represents a Python `memoryview`.
///
/// Values of this type are accessed via PyO3's smart pointers, e.g. as
/// [`Py<PyMemoryView>`][crate::Py] or [`Bound<'py, PyMemoryView>`][Bound].
#[repr(transparent)]
pub struct PyMemoryView(PyAny);

#[cfg(not(RustPython))]
pyobject_native_type_core!(PyMemoryView, pyobject_native_static_type_object!(ffi::PyMemoryView_Type), "builtins", "memoryview", #checkfunction=ffi::PyMemoryView_Check);

#[cfg(RustPython)]
pyobject_native_type_core!(
    PyMemoryView,
    |py| {
        static TYPE: PyOnceLock<Py<PyType>> = PyOnceLock::new();
        TYPE.import(py, "builtins", "memoryview").unwrap().as_type_ptr()
    },
    "builtins",
    "memoryview",
    #checkfunction=ffi::PyMemoryView_Check
);

impl PyMemoryView {
    /// Creates a new Python `memoryview` object from another Python object that
    /// implements the buffer protocol.
    pub fn from<'py>(src: &Bound<'py, PyAny>) -> PyResult<Bound<'py, Self>> {
        unsafe {
            ffi::PyMemoryView_FromObject(src.as_ptr())
                .assume_owned_or_err(src.py())
                .cast_into_unchecked()
        }
    }
}

impl<'py> TryFrom<&Bound<'py, PyAny>> for Bound<'py, PyMemoryView> {
    type Error = crate::PyErr;

    /// Creates a new Python `memoryview` object from another Python object that
    /// implements the buffer protocol.
    fn try_from(value: &Bound<'py, PyAny>) -> Result<Self, Self::Error> {
        PyMemoryView::from(value)
    }
}

#[cfg(all(any(not(Py_LIMITED_API), Py_3_11), feature = "macros"))]
mod owner_buffer {
    //! Implementation of [`PyMemoryView::from_owner`].
    //!
    //! `PyMemoryView_FromBuffer` throws away the `obj` field of the `Py_buffer`
    //! it is given, so the memoryview it returns holds no reference to the
    //! object which owns the memory (a long-standing quirk, tracked as
    //! [python/cpython#148493]). A memoryview which pointed directly into the
    //! owner's data without keeping the owner alive would therefore be
    //! unsound.
    //!
    //! To keep the owner alive, `from_owner` instead creates a small
    //! `OwnerBuffer` object which implements the buffer protocol and holds a
    //! strong reference to the owner, and then builds the memoryview from that
    //! with `PyMemoryView_FromObject`. CPython's memoryview machinery then
    //! takes its own strong reference to the `OwnerBuffer`, so the buffer (and
    //! with it the owner and its data) is guaranteed to outlive the
    //! memoryview.
    //!
    //! [python/cpython#148493]: https://github.com/python/cpython/issues/148493

    use super::PyMemoryView;
    use crate::err::{PyResult, error_on_negative};
    use crate::ffi_ptr_ext::FfiPtrExt;
    use crate::{Bound, Py, PyAny, PyClass, Python, ffi, pyclass::boolean_struct::True};
    use core::ffi::{c_int, c_void};

    impl PyMemoryView {
        /// Creates a new Python `memoryview` object which exposes a slice of
        /// memory owned by `owner`, without copying the data.
        ///
        /// `getbuf` borrows the `#[pyclass(frozen)]` value stored in `owner`
        /// and returns the bytes to expose. The higher-ranked lifetime bound
        /// guarantees that the returned slice is borrowed from `owner` (or is
        /// `'static`), so the memoryview can never outlive the data it points
        /// into.
        ///
        /// # Examples
        ///
        /// ```rust
        /// use pyo3::prelude::*;
        /// use pyo3::types::PyMemoryView;
        ///
        /// #[pyclass(frozen)]
        /// struct Bytes {
        ///     data: Vec<u8>,
        /// }
        ///
        /// # fn main() {
        /// Python::attach(|py| {
        ///     let owner = Bound::new(py, Bytes { data: vec![1, 2, 3] }).unwrap();
        ///     let view = PyMemoryView::from_owner(&owner, |bytes| &bytes.data).unwrap();
        ///     let data: Vec<u8> = view.call_method0("tobytes").unwrap().extract().unwrap();
        ///     assert_eq!(data, vec![1, 2, 3]);
        /// });
        /// # }
        /// ```
        pub fn from_owner<'py, T>(
            owner: &Bound<'py, T>,
            getbuf: impl for<'a> FnOnce(&'a T) -> &'a [u8],
        ) -> PyResult<Bound<'py, Self>>
        where
            T: PyClass<Frozen = True> + Sync,
        {
            let py = owner.py();
            let data = getbuf(owner.get());

            let bridge = OwnerBuffer::new(py, owner.clone().into_any().unbind(), data)?;

            // SAFETY: `bridge` implements the buffer protocol (its
            // `bf_getbuffer` is `OwnerBuffer::__getbuffer__`), which is all
            // that `PyMemoryView_FromObject` requires of its argument.
            unsafe {
                Ok(ffi::PyMemoryView_FromObject(bridge.as_ptr())
                    .assume_owned_or_err(py)?
                    .cast_into_unchecked())
            }
        }
    }

    /// Bridge object which implements the buffer protocol over a slice of
    /// memory belonging to another Python object, and which keeps that object
    /// alive for as long as the buffer is exported.
    #[crate::pyclass(crate = "crate", frozen)]
    struct OwnerBuffer {
        /// The Python object which owns the memory being exposed. Holding this
        /// reference is the entire point of the bridge.
        owner: Py<PyAny>,
        /// Pointer to and length of the exposed memory. These are captured
        /// once when the view is created; the memory cannot be freed (or, for
        /// a `#[pyclass(frozen)]` owner, reallocated) while `owner` is alive.
        ptr: *const u8,
        len: usize,
    }

    // SAFETY: the pointer is only dereferenced through buffer exports, which
    // outlive this object; `Py<PyAny>` is `Send + Sync` for all `T`.
    unsafe impl Send for OwnerBuffer {}
    unsafe impl Sync for OwnerBuffer {}

    impl OwnerBuffer {
        fn new<'py>(py: Python<'py>, owner: Py<PyAny>, data: &[u8]) -> PyResult<Bound<'py, Self>> {
            Bound::new(
                py,
                Self {
                    owner,
                    ptr: data.as_ptr(),
                    len: data.len(),
                },
            )
        }
    }

    #[crate::pymethods(crate = "crate")]
    impl OwnerBuffer {
        fn __traverse__(
            &self,
            visit: crate::pyclass::PyVisit<'_>,
        ) -> Result<(), crate::pyclass::PyTraverseError> {
            visit.call(&self.owner)
        }

        // Note: no `__clear__` on purpose. Dropping `owner` while a memoryview
        // holds a buffer export would let the exposed memory be freed while
        // still in use, so the bridge must keep the owner alive for its whole
        // lifetime. As a consequence a reference cycle which passes through
        // the bridge (e.g. a frozen pyclass which stores the memoryview) can
        // never be broken; this is the same trade-off PyO3 makes for any
        // `#[pyclass]` without a `__clear__`.

        unsafe fn __getbuffer__(
            slf: Bound<'_, Self>,
            view: *mut ffi::Py_buffer,
            flags: c_int,
        ) -> PyResult<()> {
            if view.is_null() {
                return Err(crate::exceptions::PyBufferError::new_err("view is null"));
            }
            if (flags & ffi::PyBUF_WRITABLE) == ffi::PyBUF_WRITABLE {
                return Err(crate::exceptions::PyBufferError::new_err(
                    "Object is not writable",
                ));
            }

            let py = slf.py();
            let obj_ptr = slf.as_ptr();
            let data = slf.get();
            unsafe {
                // SAFETY: `view` was checked to be non-null above.
                //
                // `PyBuffer_FillInfo` fills the whole view, taking a new
                // reference to the bridge object in `view.obj` which the
                // memoryview machinery will release when the buffer is
                // released.
                error_on_negative(py, {
                    ffi::PyBuffer_FillInfo(
                        view,
                        obj_ptr,
                        data.ptr as *mut c_void,
                        data.len as ffi::Py_ssize_t,
                        1, // readonly
                        ffi::PyBUF_FULL_RO,
                    )
                })?;
            }
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::PyMemoryView;
        use crate::types::IntoPyDict;
        use crate::types::any::PyAnyMethods;
        use crate::{Bound, Python};
        use alloc::string::ToString;
        use alloc::vec::Vec;
        use core::sync::atomic::{AtomicUsize, Ordering};

        #[crate::pyclass(crate = "crate", frozen)]
        struct ByteOwner {
            data: Vec<u8>,
        }

        /// Owner which counts its drops, to observe that the memoryview keeps
        /// it alive and releases it again when dropped.
        #[crate::pyclass(crate = "crate", frozen)]
        struct CountingOwner {
            data: &'static [u8],
            drops: &'static AtomicUsize,
        }

        impl Drop for CountingOwner {
            fn drop(&mut self) {
                self.drops.fetch_add(1, Ordering::Relaxed);
            }
        }

        fn get_bytes(view: &Bound<'_, crate::PyAny>) -> Vec<u8> {
            view.call_method0("tobytes").unwrap().extract().unwrap()
        }

        #[test]
        fn test_from_owner() {
            Python::attach(|py| {
                let owner = Bound::new(
                    py,
                    ByteOwner {
                        data: vec![1, 2, 3, 4, 5],
                    },
                )
                .unwrap();
                let view = PyMemoryView::from_owner(&owner, |o| &o.data).unwrap();

                assert_eq!(get_bytes(view.as_any()), vec![1, 2, 3, 4, 5]);
                assert_eq!(
                    view.getattr("nbytes").unwrap().extract::<usize>().unwrap(),
                    5
                );
                assert!(view.getattr("readonly").unwrap().extract::<bool>().unwrap());
                assert_eq!(
                    view.getattr("format").unwrap().extract::<&str>().unwrap(),
                    "B"
                );

                // a second memoryview over the same data
                let view2 = PyMemoryView::from_owner(&owner, |o| &o.data).unwrap();
                assert_eq!(get_bytes(view2.as_any()), vec![1, 2, 3, 4, 5]);

                // reading the view through Python's slicing machinery works
                let env = [("view", &view)].into_py_dict(py).unwrap();
                py.run(
                    crate::ffi::c_str!("assert bytes(view) == b'\\x01\\x02\\x03\\x04\\x05'"),
                    None,
                    Some(&env),
                )
                .unwrap();
            });
        }

        #[test]
        fn test_from_owner_empty() {
            Python::attach(|py| {
                let owner = Bound::new(py, ByteOwner { data: vec![] }).unwrap();
                let view = PyMemoryView::from_owner(&owner, |o| &o.data).unwrap();
                assert_eq!(get_bytes(view.as_any()), Vec::<u8>::new());
            });
        }

        #[test]
        fn test_from_owner_static_data() {
            Python::attach(|py| {
                let owner = Bound::new(py, ByteOwner { data: vec![] }).unwrap();
                let view = PyMemoryView::from_owner(&owner, |_| b"static bytes" as &[u8]).unwrap();
                assert_eq!(get_bytes(view.as_any()), b"static bytes".to_vec());
            });
        }

        #[test]
        fn test_from_owner_keeps_owner_alive() {
            Python::attach(|py| {
                static DROPS: AtomicUsize = AtomicUsize::new(0);

                let owner = Bound::new(
                    py,
                    CountingOwner {
                        data: b"hidden",
                        drops: &DROPS,
                    },
                )
                .unwrap();

                let view = {
                    // simulate the `Py<T>` going out of scope: the only strong
                    // references left are the memoryview's chain
                    let view = PyMemoryView::from_owner(&owner, |o| o.data).unwrap();
                    drop(owner);
                    view
                };

                // the data must still be readable after the owner `Py<T>` was
                // dropped
                assert_eq!(get_bytes(view.as_any()), b"hidden".to_vec());
                assert_eq!(DROPS.load(Ordering::Relaxed), 0);

                // dropping the memoryview must release the owner
                drop(view);
                assert_eq!(DROPS.load(Ordering::Relaxed), 1);
            });
        }

        #[test]
        fn test_from_owner_survives_gc() {
            Python::attach(|py| {
                let gc = py.import("gc").unwrap();

                let view = {
                    let owner = Bound::new(
                        py,
                        CountingOwner {
                            data: b"survive gc",
                            drops: &TEST_GC_DROPS,
                        },
                    )
                    .unwrap();
                    // only the memoryview's chain holds the owner now
                    let view = PyMemoryView::from_owner(&owner, |o| o.data).unwrap();
                    drop(owner);
                    view
                };

                // collect garbage: the bridge must be traversed safely and
                // must not be collected while the memoryview is alive
                gc.call_method0("collect").unwrap();

                assert_eq!(get_bytes(view.as_any()), b"survive gc".to_vec());
                drop(view);
                assert_eq!(TEST_GC_DROPS.load(Ordering::Relaxed), 1);
            });
        }

        static TEST_GC_DROPS: AtomicUsize = AtomicUsize::new(0);

        #[test]
        fn test_from_owner_release() {
            Python::attach(|py| {
                let owner = Bound::new(
                    py,
                    ByteOwner {
                        data: vec![7, 8, 9],
                    },
                )
                .unwrap();
                let view = PyMemoryView::from_owner(&owner, |o| &o.data).unwrap();

                assert_eq!(get_bytes(view.as_any()), vec![7, 8, 9]);

                // the buffer can be released explicitly, as with any memoryview
                view.call_method0("release").unwrap();

                let err = view.call_method0("tobytes").unwrap_err().to_string();
                assert!(
                    err.starts_with("ValueError"),
                    "expected ValueError from released memoryview, got {err}"
                );
            });
        }

        #[test]
        fn test_from_owner_read_with_pybuffer() {
            Python::attach(|py| {
                let owner = Bound::new(
                    py,
                    ByteOwner {
                        data: (0u8..100).collect(),
                    },
                )
                .unwrap();
                let view = PyMemoryView::from_owner(&owner, |o| &o.data).unwrap();

                // the memoryview is a first-class buffer-protocol object which
                // other Rust code can consume through `PyBuffer`
                let buffer = crate::buffer::PyBuffer::<u8>::get(view.as_any()).unwrap();
                assert_eq!(buffer.item_count(), 100);
                assert!(buffer.readonly());
                assert_eq!(buffer.to_vec(py).unwrap(), (0u8..100).collect::<Vec<_>>());
            });
        }

        #[test]
        fn test_from_owner_writable_request_fails() {
            Python::attach(|py| {
                let owner = Bound::new(py, ByteOwner { data: vec![1] }).unwrap();
                let view = PyMemoryView::from_owner(&owner, |o| &o.data).unwrap();

                // a writable buffer cannot be obtained from the read-only view
                let result = crate::buffer::PyUntypedBufferView::with_flags(
                    view.as_any(),
                    crate::buffer::PyBufferRequest::simple().writable(),
                    |_| unreachable!("writable request must fail"),
                );
                let err = result.unwrap_err();
                assert!(err.is_instance_of::<crate::exceptions::PyBufferError>(py));
            });
        }
    }
}
