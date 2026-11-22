//! Unpacking of Python objects into Rust tuples and arrays.

use crate::conversion::FromPyObjectOwned;
use crate::conversions::std::array_try_from_fn;
use crate::exceptions::PyValueError;
use crate::types::PyIterator;
use crate::types::any::PyAnyMethods;
use crate::{Borrowed, Bound, PyAny, PyErr, PyResult};

/// A Rust type which can be unpacked from a Python object.
///
/// This mirrors Python's unpacking syntax, e.g. `a, b, c = obj`, which accepts any iterable
/// object rather than only tuples. The trait is implemented for tuples of up to 12 elements and
/// for arrays of any size, with element types which implement [`FromPyObjectOwned`].
///
/// The [`unpack`][crate::types::any::PyAnyMethods::unpack] method of
/// [`PyAnyMethods`][crate::types::any::PyAnyMethods] is usually more convenient, because it
/// infers `Self` from the surrounding context.
///
/// # Examples
///
/// ```
/// use pyo3::prelude::*;
/// use pyo3::types::PyList;
/// use pyo3::Unpackable;
///
/// # fn main() -> PyResult<()> {
/// Python::attach(|py| {
///     let list = PyList::new(py, [1, 2, 3])?;
///     let (a, b, c) = <(i32, i32, i32) as Unpackable>::unpack(list.as_any().as_borrowed())?;
///     assert_eq!((a, b, c), (1, 2, 3));
///     Ok(())
/// })
/// # }
/// ```
///
/// Elements which borrow from the Python object are not supported, because every element is
/// extracted from a temporary reference produced by iterating the object:
///
/// ```rust,compile_fail
/// use pyo3::prelude::*;
/// use pyo3::types::PyList;
///
/// # fn main() -> PyResult<()> {
/// Python::attach(|py| {
///     let list = PyList::new(py, ["hello"])?;
///     let (s,): (&str,) = list.unpack()?;
///     # let _ = s;
///     Ok(())
/// })
/// # }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be unpacked from a Python object",
    note = "`Unpackable` is implemented for tuples and arrays of `FromPyObjectOwned` types"
)]
pub trait Unpackable<'py>: Sized {
    /// Unpacks `obj` into `Self`.
    ///
    /// This is equivalent to the Python statement `a, b, c = obj` (with `Self` being the tuple
    /// `(A, B, C)`). An error is raised if the object is not iterable or if the number of
    /// values it yields does not match the number of elements of `Self`.
    fn unpack(obj: Borrowed<'_, 'py, PyAny>) -> PyResult<Self>;
}

fn next_item<'py, T>(iter: &mut Bound<'py, PyIterator>, expected: usize, got: usize) -> PyResult<T>
where
    T: FromPyObjectOwned<'py>,
{
    match iter.next() {
        Some(Ok(item)) => item.extract::<T>().map_err(Into::into),
        Some(Err(err)) => Err(err),
        None => Err(not_enough_values(expected, got)),
    }
}

/// Checks that `iter` yields no more values after `expected` values have been unpacked.
fn ensure_exhausted(iter: &mut Bound<'_, PyIterator>, expected: usize) -> PyResult<()> {
    match iter.next() {
        None => Ok(()),
        Some(Ok(_)) => Err(too_many_values(expected)),
        // Errors raised by the iterator take precedence over the length mismatch, matching
        // Python's behaviour of pulling one extra value to detect sequences which are too long.
        Some(Err(err)) => Err(err),
    }
}

#[cold]
fn not_enough_values(expected: usize, got: usize) -> PyErr {
    PyValueError::new_err(format!(
        "not enough values to unpack (expected {expected}, got {got})"
    ))
}

#[cold]
fn too_many_values(expected: usize) -> PyErr {
    PyValueError::new_err(format!("too many values to unpack (expected {expected})"))
}

macro_rules! tuple_unpack {
    ($length:expr, $(($index:expr, $T:ident)),+) => {
        impl<'py, $($T),+> Unpackable<'py> for ($($T,)+)
        where
            $($T: FromPyObjectOwned<'py>,)+
        {
            fn unpack(obj: Borrowed<'_, 'py, PyAny>) -> PyResult<Self> {
                let mut iter = obj.try_iter()?;
                // Tuple elements are evaluated from left to right, so the values are unpacked
                // in the same order as they appear in the tuple.
                let out = ($(next_item::<$T>(&mut iter, $length, $index)?,)+);
                ensure_exhausted(&mut iter, $length)?;
                Ok(out)
            }
        }
    };
}

tuple_unpack!(1, (0, T0));
tuple_unpack!(2, (0, T0), (1, T1));
tuple_unpack!(3, (0, T0), (1, T1), (2, T2));
tuple_unpack!(4, (0, T0), (1, T1), (2, T2), (3, T3));
tuple_unpack!(5, (0, T0), (1, T1), (2, T2), (3, T3), (4, T4));
tuple_unpack!(6, (0, T0), (1, T1), (2, T2), (3, T3), (4, T4), (5, T5));
tuple_unpack!(
    7,
    (0, T0),
    (1, T1),
    (2, T2),
    (3, T3),
    (4, T4),
    (5, T5),
    (6, T6)
);
tuple_unpack!(
    8,
    (0, T0),
    (1, T1),
    (2, T2),
    (3, T3),
    (4, T4),
    (5, T5),
    (6, T6),
    (7, T7)
);
tuple_unpack!(
    9,
    (0, T0),
    (1, T1),
    (2, T2),
    (3, T3),
    (4, T4),
    (5, T5),
    (6, T6),
    (7, T7),
    (8, T8)
);
tuple_unpack!(
    10,
    (0, T0),
    (1, T1),
    (2, T2),
    (3, T3),
    (4, T4),
    (5, T5),
    (6, T6),
    (7, T7),
    (8, T8),
    (9, T9)
);
tuple_unpack!(
    11,
    (0, T0),
    (1, T1),
    (2, T2),
    (3, T3),
    (4, T4),
    (5, T5),
    (6, T6),
    (7, T7),
    (8, T8),
    (9, T9),
    (10, T10)
);
tuple_unpack!(
    12,
    (0, T0),
    (1, T1),
    (2, T2),
    (3, T3),
    (4, T4),
    (5, T5),
    (6, T6),
    (7, T7),
    (8, T8),
    (9, T9),
    (10, T10),
    (11, T11)
);

impl<'py, T, const N: usize> Unpackable<'py> for [T; N]
where
    T: FromPyObjectOwned<'py>,
{
    fn unpack(obj: Borrowed<'_, 'py, PyAny>) -> PyResult<Self> {
        let mut iter = obj.try_iter()?;
        let array = array_try_from_fn(|index| next_item::<T>(&mut iter, N, index))?;
        ensure_exhausted(&mut iter, N)?;
        Ok(array)
    }
}

#[cfg(test)]
mod tests {
    use super::Unpackable;
    use crate::platform::prelude::*;
    use crate::types::PyList;
    use crate::{Python, exceptions};

    #[test]
    fn test_unpack_array_from_list() {
        Python::attach(|py| {
            let list = PyList::new(py, [1, 2, 3]).unwrap();
            let array = <[i32; 3] as Unpackable>::unpack(list.as_any().as_borrowed()).unwrap();
            assert_eq!(array, [1, 2, 3]);
        });
    }

    #[test]
    fn test_unpack_array_too_many_values() {
        Python::attach(|py| {
            let list = PyList::new(py, [1, 2, 3]).unwrap();
            let err = <[i32; 2] as Unpackable>::unpack(list.as_any().as_borrowed()).unwrap_err();
            assert!(err.is_instance_of::<exceptions::PyValueError>(py));
            assert_eq!(
                err.value(py).to_string(),
                "too many values to unpack (expected 2)"
            );
        });
    }

    #[test]
    fn test_unpack_non_iterable() {
        Python::attach(|py| {
            let obj = py.eval(c"42", None, None).unwrap();
            let err = <(i32, i32) as Unpackable>::unpack(obj.as_borrowed()).unwrap_err();
            assert!(err.is_instance_of::<exceptions::PyTypeError>(py));
        });
    }
}
