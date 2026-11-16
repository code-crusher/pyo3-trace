use pyo3::prelude::*;

#[pyclass(subclass)]
struct Base;

#[pyclass(extends = Base, subclass)]
struct Child(u64);

#[pymethods]
//~^ ERROR: `Child` cannot be converted to a Python object
impl Child {
    #[new]
    fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(Base).add_subclass(Self(0))
    }

    fn __next__(&mut self) -> Self {
        self.0 += 1;
        Self(self.0)
    }
}

fn main() {}