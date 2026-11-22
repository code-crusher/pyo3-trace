//! Tests for [`pyo3::Unpackable`] and [`pyo3::types::any::PyAnyMethods::unpack`].
#![cfg(feature = "macros")]

use pyo3::Unpackable;
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyString, PyTuple};

mod test_utils;

#[test]
fn test_unpack_tuple() {
    Python::attach(|py| {
        let tuple = PyTuple::new(py, [1, 2, 3]).unwrap();
        let unpacked: (i32, i32, i32) = tuple.unpack().unwrap();
        assert_eq!(unpacked, (1, 2, 3));
    });
}

#[test]
fn test_unpack_sequence_types() {
    Python::attach(|py| {
        // Lists and ranges implement the sequence protocol.
        let list = PyList::new(py, [1, 2, 3]).unwrap();
        let (a, b, c): (i32, i32, i32) = list.unpack().unwrap();
        assert_eq!((a, b, c), (1, 2, 3));

        let range = py.eval(c"range(3)", None, None).unwrap();
        let (a, b, c): (i32, i32, i32) = range.unpack().unwrap();
        assert_eq!((a, b, c), (0, 1, 2));
    });
}

#[test]
fn test_unpack_str() {
    Python::attach(|py| {
        // Strings are iterable, so unpacking them yields their characters.
        let s = PyString::new(py, "abc");
        let (a, b, c): (String, String, String) = s.unpack().unwrap();
        assert_eq!((a.as_str(), b.as_str(), c.as_str()), ("a", "b", "c"));
    });
}

#[test]
fn test_unpack_iterators_and_generators() {
    Python::attach(|py| {
        // Any iterable can be unpacked, not only sequences.
        let set = PySet::new(py, [42]).unwrap();
        let (value,): (i32,) = set.unpack().unwrap();
        assert_eq!(value, 42);

        let generator = py.eval(c"(x * 2 for x in range(3))", None, None).unwrap();
        let (a, b, c): (i32, i32, i32) = generator.unpack().unwrap();
        assert_eq!((a, b, c), (0, 2, 4));

        // Dicts iterate over their keys.
        let dict = PyDict::new(py);
        dict.set_item("key", 1).unwrap();
        let (key,): (String,) = dict.unpack().unwrap();
        assert_eq!(key, "key");
    });
}

#[test]
fn test_unpack_nested() {
    Python::attach(|py| {
        // Unpacking applies to the outermost object; nested elements are converted using
        // `FromPyObject`, so they must still be tuples for a tuple element type.
        let list = py.eval(c"[(1, 2), (3, 4)]", None, None).unwrap();
        let ((a, b), (c, d)): ((i32, i32), (i32, i32)) = list.unpack().unwrap();
        assert_eq!((a, b, c, d), (1, 2, 3, 4));

        // A list of lists is not unpacked recursively.
        let list_of_lists = PyList::new(py, [[1, 2], [3, 4]]).unwrap();
        assert!(list_of_lists.unpack::<((i32, i32), (i32, i32))>().is_err());
    });
}

#[test]
fn test_unpack_array() {
    Python::attach(|py| {
        let list = PyList::new(py, [1, 2, 3]).unwrap();
        let array: [i32; 3] = list.unpack().unwrap();
        assert_eq!(array, [1, 2, 3]);

        // Arrays of zero length only accept empty iterables.
        let empty: [i32; 0] = PyList::empty(py).unpack().unwrap();
        assert_eq!(empty.len(), 0);
    });
}

#[test]
fn test_unpack_trait_method() {
    Python::attach(|py| {
        let list = PyList::new(py, [1, 2, 3]).unwrap();
        let unpacked = <(i32, i32, i32) as Unpackable>::unpack(list.as_any().as_borrowed());
        assert_eq!(unpacked.unwrap(), (1, 2, 3));
    });
}

#[test]
fn test_unpack_not_enough_values() {
    Python::attach(|py| {
        let list = PyList::new(py, [1, 2]).unwrap();
        let err = list.unpack::<(i32, i32, i32)>().unwrap_err();
        assert!(err.is_instance_of::<PyValueError>(py));
        assert_eq!(
            err.value(py).to_string(),
            "not enough values to unpack (expected 3, got 2)"
        );

        // Nothing at all is also reported as a length mismatch rather than an error.
        let empty = PyList::empty(py);
        let err = empty.unpack::<(i32,)>().unwrap_err();
        assert!(err.is_instance_of::<PyValueError>(py));
        assert_eq!(
            err.value(py).to_string(),
            "not enough values to unpack (expected 1, got 0)"
        );
    });
}

#[test]
fn test_unpack_too_many_values() {
    Python::attach(|py| {
        let list = PyList::new(py, [1, 2, 3]).unwrap();
        let err = list.unpack::<(i32, i32)>().unwrap_err();
        assert!(err.is_instance_of::<PyValueError>(py));
        assert_eq!(
            err.value(py).to_string(),
            "too many values to unpack (expected 2)"
        );

        let err = PyList::new(py, [1])
            .unwrap()
            .unpack::<[i32; 0]>()
            .unwrap_err();
        assert!(err.is_instance_of::<PyValueError>(py));
        assert_eq!(
            err.value(py).to_string(),
            "too many values to unpack (expected 0)"
        );
    });
}

#[test]
fn test_unpack_non_iterable() {
    Python::attach(|py| {
        let err = py
            .eval(c"42", None, None)
            .unwrap()
            .unpack::<(i32, i32)>()
            .unwrap_err();
        assert!(err.is_instance_of::<PyTypeError>(py));
    });
}

#[test]
fn test_unpack_element_extraction_error() {
    Python::attach(|py| {
        let list = py.eval(c"[1, 'two']", None, None).unwrap();
        let err = list.unpack::<(i32, i32)>().unwrap_err();
        assert!(err.is_instance_of::<PyTypeError>(py));
    });
}

#[test]
fn test_unpack_element_error_before_length_error() {
    Python::attach(|py| {
        // A bad element is reported before any length mismatch, like Python's own unpacking
        // which fails on the first offending value.
        let list = PyList::new(py, ["a", "b", "c"]).unwrap();
        let err = list.unpack::<(String, i32)>().unwrap_err();
        assert!(err.is_instance_of::<PyTypeError>(py));
    });
}

#[test]
fn test_unpack_propagates_iterator_exception() {
    Python::attach(|py| {
        py.run(
            cr#"
class RaisingIterator:
    def __iter__(self):
        return self

    def __next__(self):
        raise RuntimeError("iterator failed")
"#,
            None,
            None,
        )
        .unwrap();
        let obj = py.eval(c"RaisingIterator()", None, None).unwrap();
        let err = obj.unpack::<(i32,)>().unwrap_err();
        assert!(err.is_instance_of::<PyRuntimeError>(py));
        assert_eq!(err.value(py).to_string(), "iterator failed");
    });
}

#[test]
fn test_unpack_does_not_use_len() {
    Python::attach(|py| {
        // `__len__` returning a nonsensical value must not affect unpacking, matching Python.
        py.run(
            cr#"
class LyingLength:
    def __len__(self):
        return 99

    def __getitem__(self, index):
        if index < 2:
            return index
        raise IndexError
"#,
            None,
            None,
        )
        .unwrap();
        let obj = py.eval(c"LyingLength()", None, None).unwrap();
        let (a, b): (i32, i32) = obj.unpack().unwrap();
        assert_eq!((a, b), (0, 1));

        let err = obj.unpack::<(i32, i32, i32)>().unwrap_err();
        assert!(err.is_instance_of::<PyValueError>(py));
        assert_eq!(
            err.value(py).to_string(),
            "not enough values to unpack (expected 3, got 2)"
        );
    });
}

#[test]
fn test_unpack_getitem_only() {
    Python::attach(|py| {
        // Objects implementing only `__getitem__` are iterable via the legacy protocol.
        py.run(
            cr#"
class GetItemOnly:
    def __getitem__(self, index):
        if index < 3:
            return index
        raise IndexError
"#,
            None,
            None,
        )
        .unwrap();
        let obj = py.eval(c"GetItemOnly()", None, None).unwrap();
        let (a, b, c): (i32, i32, i32) = obj.unpack().unwrap();
        assert_eq!((a, b, c), (0, 1, 2));
    });
}

#[test]
fn test_unpack_large_tuple() {
    Python::attach(|py| {
        let list = PyList::new(py, 0..12).unwrap();
        let unpacked: (i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) =
            list.unpack().unwrap();
        assert_eq!(unpacked, (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11));
    });
}

#[test]
fn test_unpack_into_bound_elements() {
    Python::attach(|py| {
        // Unpacked values may themselves be Python objects.
        let list = PyList::new(py, [[1, 2], [3, 4]]).unwrap();
        let (a, b): (Bound<'_, PyList>, Bound<'_, PyList>) = list.unpack().unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
    });
}
