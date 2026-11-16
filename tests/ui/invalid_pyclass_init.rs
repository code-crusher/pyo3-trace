use pyo3::prelude::*;

#[pyclass]
struct InvalidInitReturn;

#[pymethods]
impl InvalidInitReturn {
    fn __init__(&self) -> i32 {
        //~^ ERROR: `i32` cannot be converted to a Python object
        0
    }
}

fn main() {}
