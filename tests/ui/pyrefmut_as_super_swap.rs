use pyo3::prelude::*;

#[pyclass(subclass)]
struct BaseClass {
    val1: usize,
}

#[pyclass(extends = BaseClass, subclass)]
struct SubClass {
    val2: usize,
}

// Exposing the guarded `PyRefMut<T::BaseType>` directly would allow swapping it with
// a guard borrowed from a plain instance of the base type, causing type confusion.
// `as_super` therefore returns `PyRefMutSuper`, which does not expose the inner guard.
fn swap_with_base_guards(
    mut sub: PyRefMut<'_, SubClass>,
    base: &Bound<'_, BaseClass>,
) {
    std::mem::swap(
        sub.as_super(), //~ERROR: mismatched types
        &mut base.borrow_mut(),
    );
}

fn main() {}
