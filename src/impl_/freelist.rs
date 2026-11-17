//! Support for [free allocation lists][1].
//!
//! This can improve performance for types that are often created and deleted in quick succession.
//!
//! Rather than implementing this manually,
//! implement it by annotating a struct with `#[pyclass(freelist = N)]`,
//! where `N` is the size of the freelist.
//!
//! On GIL-enabled builds the freelist is not synchronized. `tp_alloc` and `tp_free` are only
//! ever called with the thread attached to the interpreter, which implies that the calling
//! thread holds the GIL, so all accesses to a freelist are already serialized.
//!
//! On free-threaded builds the freelist is protected by a mutex, since multiple threads may
//! allocate and deallocate instances of the same class concurrently.
//!
//! [1]: https://en.wikipedia.org/wiki/Free_list

use crate::ffi;
use crate::marker::Python;
use crate::platform::prelude::*;
#[cfg(Py_GIL_DISABLED)]
use crate::platform::sync::nonpoison::Mutex;
#[cfg(Py_GIL_DISABLED)]
use crate::sync::MutexExt;
use crate::sync::PyOnceLock;

use core::ops::DerefMut;
use core::ptr::NonNull;

#[cfg(not(Py_GIL_DISABLED))]
use core::cell::UnsafeCell;

pub struct FreeList(PyOnceLock<FreelistState>);

impl FreeList {
    #[expect(clippy::new_without_default, reason = "always called in const context")]
    pub const fn new() -> Self {
        Self(PyOnceLock::new())
    }

    pub fn get(
        &self,
        py: Python<'_>,
        capacity: usize,
    ) -> impl DerefMut<Target = PyObjectFreeList> + use<'_> {
        let state = self.0.get_or_init(py, || {
            FreelistState::new(PyObjectFreeList::with_capacity(capacity))
        });

        #[cfg(Py_GIL_DISABLED)]
        {
            state.0.lock_py_attached(py)
        }
        #[cfg(not(Py_GIL_DISABLED))]
        {
            // SAFETY: `tp_alloc` and `tp_free` are only called with the thread attached to the
            // interpreter, which means the GIL is held. All accesses to the freelist are
            // therefore serialized, and no Python code is executed while the returned reference
            // is alive, so no other thread can observe or mutate the freelist concurrently.
            unsafe { &mut *state.0.get() }
        }
    }
}

/// State stored in the `PyOnceLock` for free-threaded builds.
///
/// Access is synchronized by the mutex.
#[cfg(Py_GIL_DISABLED)]
struct FreelistState(Mutex<PyObjectFreeList>);

#[cfg(Py_GIL_DISABLED)]
impl FreelistState {
    fn new(list: PyObjectFreeList) -> Self {
        Self(Mutex::new(list))
    }
}

/// State stored in the `PyOnceLock` for GIL-enabled builds.
///
/// Access is serialized by the GIL, so no mutex is required.
#[cfg(not(Py_GIL_DISABLED))]
struct FreelistState(UnsafeCell<PyObjectFreeList>);

// SAFETY: on GIL-enabled builds `tp_alloc` and `tp_free` are only called with the interpreter
// attached and the GIL held. The GIL serializes all accesses to the freelist, so no two threads
// can access the `UnsafeCell` concurrently. (Free-threaded builds use the mutex-based state
// above instead.)
#[cfg(not(Py_GIL_DISABLED))]
unsafe impl Sync for FreelistState {}

#[cfg(not(Py_GIL_DISABLED))]
impl FreelistState {
    const fn new(list: PyObjectFreeList) -> Self {
        Self(UnsafeCell::new(list))
    }
}

/// A free allocation list for PyObject ffi pointers.
///
/// See [the parent module](crate::impl_::freelist) for more details.
pub struct PyObjectFreeList {
    entries: Box<[Option<NonNull<ffi::PyObject>>]>,
    split: usize,
}

// safety: the pointers are never used internally and they are cleared when they are given out
unsafe impl Send for PyObjectFreeList {}

impl PyObjectFreeList {
    /// Creates a new `PyObjectFreeList` instance with specified capacity.
    ///
    /// Note: at most `capacity - 1` pointers can be stored; see [`Self::insert`].
    pub fn with_capacity(capacity: usize) -> PyObjectFreeList {
        let entries = vec![None; capacity].into_boxed_slice();

        PyObjectFreeList { entries, split: 0 }
    }

    /// Pops the first non empty item.
    pub fn pop(&mut self) -> Option<NonNull<ffi::PyObject>> {
        let idx = self.split;
        if idx == 0 {
            None
        } else {
            let val = self.entries[idx - 1]
                .take()
                .expect("PyObjectFreeList is corrupt");
            self.split = idx - 1;
            Some(val)
        }
    }

    /// Inserts a value into the list. Returns `Some(val)` if the `PyObjectFreeList` is full.
    pub fn insert(&mut self, val: NonNull<ffi::PyObject>) -> Option<NonNull<ffi::PyObject>> {
        let next = self.split + 1;
        if next < self.entries.len() {
            self.entries[self.split] = Some(val);
            self.split = next;
            None
        } else {
            Some(val)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sentinel pointers for testing. Never dereferenced.
    fn fake_ptr(n: usize) -> NonNull<ffi::PyObject> {
        NonNull::new(core::ptr::without_provenance_mut(16 * (n + 1))).unwrap()
    }

    #[test]
    fn py_object_free_list_pop_empty() {
        let mut list = PyObjectFreeList::with_capacity(2);
        assert_eq!(list.pop(), None);
    }

    #[test]
    fn py_object_free_list_insert_and_pop_lifo() {
        // Note: an instance created with `with_capacity(n)` stores at most `n - 1`
        // pointers; the check in `insert` keeps one slot permanently unused.
        let mut list = PyObjectFreeList::with_capacity(3);
        let a = fake_ptr(0);
        let b = fake_ptr(1);
        let c = fake_ptr(2);

        assert_eq!(list.insert(a), None);
        assert_eq!(list.insert(b), None);
        assert_eq!(list.insert(c), Some(c));

        assert_eq!(list.pop(), Some(b));
        assert_eq!(list.pop(), Some(a));
        assert_eq!(list.pop(), None);
    }

    #[test]
    fn py_object_free_list_full_insert_returns_value() {
        // With `with_capacity(2)` only one pointer can be stored.
        let mut list = PyObjectFreeList::with_capacity(2);
        let a = fake_ptr(0);
        let b = fake_ptr(1);

        assert_eq!(list.insert(a), None);
        assert_eq!(list.insert(b), Some(b));

        assert_eq!(list.pop(), Some(a));
        assert_eq!(list.pop(), None);
    }

    #[test]
    fn free_list_initializes_once_and_reuses_storage() {
        Python::attach(|py| {
            let free_list = FreeList::new();
            let a = fake_ptr(0);

            assert_eq!(free_list.get(py, 2).insert(a), None);
            // A second `get` call returns the same underlying freelist,
            // observing the entry inserted above.
            assert_eq!(free_list.get(py, 2).pop(), Some(a));
            assert_eq!(free_list.get(py, 2).pop(), None);
        });
    }
}
