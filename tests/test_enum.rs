#![cfg(feature = "macros")]

use pyo3::prelude::*;
use pyo3::py_run;
use pyo3::types::PyString;

mod test_utils;

// `Copy` + `from_py_object` is a regression test for `clippy::clone_on_copy`
// firing in the generated `FromPyObject` implementation (#6308)
#[pyclass(eq, eq_int, from_py_object)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MyEnum {
    Variant,
    OtherVariant,
}

#[test]
fn test_enum_class_attr() {
    Python::attach(|py| {
        let my_enum = py.get_type::<MyEnum>();
        let var = Py::new(py, MyEnum::Variant).unwrap();
        py_assert!(py, my_enum var, "my_enum.Variant == var");
    })
}

#[test]
fn test_enum_intopyobject() {
    // Variant of the above that goes via `.into_pyobject()`
    // - regression test for https://github.com/PyO3/pyo3/issues/5927
    //
    #[pyclass(eq, eq_int, from_py_object)]
    #[derive(Debug, PartialEq, Eq, Clone)]
    pub enum MyEnum {
        A,
    }

    Python::attach(|py| {
        let var = MyEnum::A.into_pyobject(py).unwrap();
        // NB important to call this after `into_pyobject` - this ensures
        // that `.into_pyobject()` has to lazily initialize the enum type object
        let my_enum = py.get_type::<MyEnum>();
        py_assert!(py, my_enum var, "my_enum.A == var");
    });
}

#[test]
fn test_enum_eq_enum() {
    Python::attach(|py| {
        let var1 = Py::new(py, MyEnum::Variant).unwrap();
        let var2 = Py::new(py, MyEnum::Variant).unwrap();
        let other_var = Py::new(py, MyEnum::OtherVariant).unwrap();
        py_assert!(py, var1 var2, "var1 == var2");
        py_assert!(py, var1 other_var, "var1 != other_var");
        py_assert!(py, var1 var2, "(var1 != var2) == False");
    })
}

#[test]
fn test_enum_eq_incomparable() {
    Python::attach(|py| {
        let var1 = Py::new(py, MyEnum::Variant).unwrap();
        py_assert!(py, var1, "(var1 == 'foo') == False");
        py_assert!(py, var1, "(var1 != 'foo') == True");
    })
}

#[pyfunction]
fn return_enum() -> MyEnum {
    MyEnum::Variant
}

#[test]
fn test_return_enum() {
    Python::attach(|py| {
        let f = wrap_pyfunction!(return_enum)(py).unwrap();
        let mynum = py.get_type::<MyEnum>();

        py_run!(py, f mynum, "assert f() == mynum.Variant");
        py_run!(py, f mynum, "assert f() is mynum.Variant");
    });
}

#[pyfunction]
fn enum_arg(e: MyEnum) {
    assert_eq!(MyEnum::OtherVariant, e)
}

#[test]
fn test_enum_arg() {
    Python::attach(|py| {
        let f = wrap_pyfunction!(enum_arg)(py).unwrap();
        let mynum = py.get_type::<MyEnum>();

        py_run!(py, f mynum, "f(mynum.OtherVariant)")
    })
}

#[pyclass(eq, eq_int)]
#[derive(Debug, PartialEq, Eq, Clone)]
enum CustomDiscriminant {
    One = 1,
    Two = 2,
}

#[test]
fn test_custom_discriminant() {
    Python::attach(|py| {
        #[allow(non_snake_case)]
        let CustomDiscriminant = py.get_type::<CustomDiscriminant>();
        let one = Py::new(py, CustomDiscriminant::One).unwrap();
        let two = Py::new(py, CustomDiscriminant::Two).unwrap();
        py_run!(py, CustomDiscriminant one two, r#"
        assert CustomDiscriminant.One == one
        assert CustomDiscriminant.Two == two
        assert CustomDiscriminant.One == 1
        assert CustomDiscriminant.Two == 2
        assert one != two
        assert CustomDiscriminant.One != 2
        assert CustomDiscriminant.Two != 1
        "#);
    })
}

#[test]
fn test_enum_to_int() {
    Python::attach(|py| {
        let one = Py::new(py, CustomDiscriminant::One).unwrap();
        py_assert!(py, one, "int(one) == 1");
        let v = Py::new(py, MyEnum::Variant).unwrap();
        let v_value = MyEnum::Variant as isize;
        py_run!(py, v v_value, "int(v) == v_value");
    })
}

#[test]
fn test_enum_compare_int() {
    Python::attach(|py| {
        let one = Py::new(py, CustomDiscriminant::One).unwrap();
        py_run!(
            py,
            one,
            r#"
            assert one == 1
            assert 1 == one
            assert one != 2
        "#
        )
    })
}

#[pyclass(eq, eq_int)]
#[derive(Debug, PartialEq, Eq, Clone)]
#[repr(u8)]
enum SmallEnum {
    V = 1,
}

#[test]
fn test_enum_compare_int_no_throw_when_overflow() {
    Python::attach(|py| {
        let v = Py::new(py, SmallEnum::V).unwrap();
        py_assert!(py, v, "v != 1<<30")
    })
}

#[pyclass(eq, eq_int)]
#[derive(Debug, PartialEq, Eq, Clone)]
#[repr(usize)]
#[allow(clippy::enum_clike_unportable_variant)]
enum BigEnum {
    V = usize::MAX,
}

#[test]
fn test_big_enum_no_overflow() {
    Python::attach(|py| {
        let usize_max = usize::MAX;
        let v = Py::new(py, BigEnum::V).unwrap();

        py_assert!(py, usize_max v, "v == usize_max");
        py_assert!(py, usize_max v, "int(v) == usize_max");
    })
}

#[pyclass(eq, eq_int)]
#[derive(Debug, PartialEq, Eq, Clone)]
#[repr(u16, align(8))]
enum TestReprParse {
    V,
}

#[test]
fn test_repr_parse() {
    assert_eq!(std::mem::align_of::<TestReprParse>(), 8);
}

#[pyclass(eq, eq_int, name = "MyEnum")]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RenameEnum {
    Variant,
}

#[test]
fn test_rename_enum_repr_correct() {
    Python::attach(|py| {
        let var1 = Py::new(py, RenameEnum::Variant).unwrap();
        py_assert!(py, var1, "repr(var1) == 'MyEnum.Variant'");
    })
}

#[pyclass(eq, eq_int)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RenameVariantEnum {
    #[pyo3(name = "VARIANT")]
    Variant,
}

#[test]
fn test_rename_variant_repr_correct() {
    Python::attach(|py| {
        let var1 = Py::new(py, RenameVariantEnum::Variant).unwrap();
        py_assert!(py, var1, "repr(var1) == 'RenameVariantEnum.VARIANT'");
    })
}

#[pyclass(eq, eq_int, rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Debug, PartialEq, Eq, Clone)]
#[expect(clippy::enum_variant_names)]
enum RenameAllVariantsEnum {
    VariantOne,
    VariantTwo,
    #[pyo3(name = "VariantThree")]
    VariantFour,
}

#[test]
fn test_renaming_all_enum_variants() {
    Python::attach(|py| {
        let enum_obj = py.get_type::<RenameAllVariantsEnum>();
        py_assert!(py, enum_obj, "enum_obj.VARIANT_ONE == enum_obj.VARIANT_ONE");
        py_assert!(py, enum_obj, "enum_obj.VARIANT_TWO == enum_obj.VARIANT_TWO");
        py_assert!(
            py,
            enum_obj,
            "enum_obj.VariantThree == enum_obj.VariantThree"
        );
    });
}

#[pyclass(module = "custom_module")]
#[derive(Debug)]
enum CustomModuleComplexEnum {
    Variant(),
    Py(Py<PyAny>),
}

#[test]
fn test_custom_module() {
    Python::attach(|py| {
        let enum_obj = py.get_type::<CustomModuleComplexEnum>();
        py_assert!(
            py,
            enum_obj,
            "enum_obj.Variant.__module__ == 'custom_module'"
        );
    });
}

#[pyclass(eq)]
#[derive(Debug, Clone, PartialEq)]
pub enum EqOnly {
    VariantA,
    VariantB,
}

#[test]
fn test_simple_enum_eq_only() {
    Python::attach(|py| {
        let var1 = Py::new(py, EqOnly::VariantA).unwrap();
        let var2 = Py::new(py, EqOnly::VariantA).unwrap();
        let var3 = Py::new(py, EqOnly::VariantB).unwrap();
        py_assert!(py, var1 var2, "var1 == var2");
        py_assert!(py, var1 var3, "var1 != var3");
    })
}

#[cfg(wip_feature_std)]
#[pyclass(frozen, eq, eq_int, hash)]
#[derive(PartialEq, Hash)]
enum SimpleEnumWithHash {
    A,
    B,
}

#[test]
#[cfg(wip_feature_std)]
fn test_simple_enum_with_hash() {
    Python::attach(|py| {
        use pyo3::types::IntoPyDict;
        let class = SimpleEnumWithHash::A;
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            class.hash(&mut hasher);
            hasher.finish() as isize
        };

        let env = [
            ("obj", Py::new(py, class).unwrap().into_any()),
            ("hsh", hash.into_pyobject(py).unwrap().into_any().unbind()),
        ]
        .into_py_dict(py)
        .unwrap();

        py_assert!(py, *env, "hash(obj) == hsh");
    });
}

#[cfg(wip_feature_std)]
#[pyclass(eq, hash)]
#[derive(PartialEq, Hash)]
enum ComplexEnumWithHash {
    A(u32),
    B { msg: String },
}

#[test]
#[cfg(wip_feature_std)]
fn test_complex_enum_with_hash() {
    Python::attach(|py| {
        use pyo3::types::IntoPyDict;
        let class = ComplexEnumWithHash::B {
            msg: String::from("Hello"),
        };
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            class.hash(&mut hasher);
            hasher.finish() as isize
        };

        let env = [
            ("obj", Py::new(py, class).unwrap().into_any()),
            ("hsh", hash.into_pyobject(py).unwrap().into_any().unbind()),
        ]
        .into_py_dict(py)
        .unwrap();

        py_assert!(py, *env, "hash(obj) == hsh");
    });
}

#[test]
fn custom_eq() {
    #[pyclass(frozen)]
    #[derive(PartialEq)]
    pub enum CustomPyEq {
        A,
        B,
    }

    #[pymethods]
    impl CustomPyEq {
        fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
            if let Ok(rhs) = other.cast::<PyString>() {
                rhs.to_cow().is_ok_and(|rhs| self.__str__() == rhs)
            } else if let Ok(rhs) = other.cast::<Self>() {
                self == rhs.get()
            } else {
                false
            }
        }

        fn __str__(&self) -> String {
            match self {
                CustomPyEq::A => "A".to_string(),
                CustomPyEq::B => "B".to_string(),
            }
        }
    }

    Python::attach(|py| {
        let a = Bound::new(py, CustomPyEq::A).unwrap();
        let b = Bound::new(py, CustomPyEq::B).unwrap();

        assert!(a.as_any().eq(&a).unwrap());
        assert!(a.as_any().eq("A").unwrap());
        assert!(a.as_any().ne(&b).unwrap());
        assert!(a.as_any().ne("B").unwrap());

        assert!(b.as_any().eq(&b).unwrap());
        assert!(b.as_any().eq("B").unwrap());
        assert!(b.as_any().ne(&a).unwrap());
        assert!(b.as_any().ne("A").unwrap());
    })
}

#[pyclass]
#[derive(Clone, Copy)]
pub enum ComplexEnumWithRaw {
    Raw { r#type: i32 },
}

// Cover simple field lookups with raw identifiers
#[test]
fn complex_enum_with_raw() {
    Python::attach(|py| {
        let complex = ComplexEnumWithRaw::Raw { r#type: 314159 };

        py_assert!(py, complex, "complex.type == 314159");
    });
}

// Cover pattern matching with raw identifiers
#[test]
#[cfg(Py_3_10)]
fn complex_enum_with_raw_pattern_match() {
    Python::attach(|py| {
        let complex = ComplexEnumWithRaw::Raw { r#type: 314159 };
        let cls = py.get_type::<ComplexEnumWithRaw>();

        // Cover destructuring by pattern matching
        py_run!(py, cls complex, r#"
        match complex:
            case cls.Raw(type=ty):
                assert ty == 314159
            case _:
                assert False, "no matching variant found"
        "#);
    });
}

#[test]
fn complex_enum_variant_qualname() {
    #[pyclass]
    pub enum ComplexEnum {
        A(i32),
        B { msg: String },
    }

    Python::attach(|py| {
        let cls = py.get_type::<ComplexEnum>();
        py_assert!(py, cls, "cls.A.__qualname__ == 'ComplexEnum.A'");
        py_assert!(py, cls, "cls.B.__qualname__ == 'ComplexEnum.B'");
    });
}

#[test]
fn complex_enum_renamed_variant_qualname() {
    #[pyclass(name = "ComplexEnum")]
    pub enum PyComplexEnum {
        #[pyo3(name = "A")]
        PyA(i32),
        B {
            msg: String,
        },
    }

    Python::attach(|py| {
        let cls = py.get_type::<PyComplexEnum>();
        py_assert!(py, cls, "cls.A.__qualname__ == 'ComplexEnum.A'");
        py_assert!(py, cls, "cls.B.__qualname__ == 'ComplexEnum.B'");
    });
}

// Manual equivalent of the variant classes which `#[pyclass]` generates for a
// complex enum, for the case where the enum comes from another crate and so
// cannot be annotated directly. See the "Emulating variant classes for external
// enums" section of the guide.
mod external_enum {
    use super::*;
    use pyo3::types::{PyTuple, PyType};

    #[derive(Debug, PartialEq)]
    pub enum ExternalShape {
        Circle { radius: f64 },
        Rectangle { width: f64, height: f64 },
    }

    #[pyclass(subclass, name = "ExternalShape")]
    pub struct PyExternalShape(pub ExternalShape);

    #[pymethods]
    impl PyExternalShape {
        #[classattr]
        #[allow(non_snake_case)]
        fn Circle(py: Python<'_>) -> Bound<'_, PyType> {
            py.get_type::<PyExternalShapeCircle>()
        }

        #[classattr]
        #[allow(non_snake_case)]
        fn Rectangle(py: Python<'_>) -> Bound<'_, PyType> {
            py.get_type::<PyExternalShapeRectangle>()
        }
    }

    impl PyExternalShape {
        pub fn from_shape(shape: ExternalShape, py: Python<'_>) -> PyResult<Py<PyExternalShape>> {
            let value = match shape {
                ExternalShape::Circle { radius } => Py::new(
                    py,
                    PyClassInitializer::from(PyExternalShape(ExternalShape::Circle { radius }))
                        .add_subclass(PyExternalShapeCircle),
                )?
                .into_any(),
                ExternalShape::Rectangle { width, height } => Py::new(
                    py,
                    PyClassInitializer::from(PyExternalShape(ExternalShape::Rectangle {
                        width,
                        height,
                    }))
                    .add_subclass(PyExternalShapeRectangle),
                )?
                .into_any(),
            };
            Ok(value
                .into_bound(py)
                .cast_into::<PyExternalShape>()?
                .unbind())
        }
    }

    #[pyclass(extends = PyExternalShape, name = "ExternalShapeCircle")]
    pub struct PyExternalShapeCircle;

    #[pymethods]
    impl PyExternalShapeCircle {
        #[new]
        fn new(radius: f64) -> PyClassInitializer<Self> {
            PyClassInitializer::from(PyExternalShape(ExternalShape::Circle { radius }))
                .add_subclass(Self)
        }

        #[classattr]
        fn __match_args__(py: Python<'_>) -> PyResult<Bound<'_, PyTuple>> {
            PyTuple::new(py, ["radius"])
        }

        #[getter]
        fn radius(self_: PyRef<'_, Self>) -> f64 {
            match self_.as_ref().0 {
                ExternalShape::Circle { radius } => radius,
                _ => unreachable!("wrong variant"),
            }
        }
    }

    #[pyclass(extends = PyExternalShape, name = "ExternalShapeRectangle")]
    pub struct PyExternalShapeRectangle;

    #[pymethods]
    impl PyExternalShapeRectangle {
        #[new]
        fn new(width: f64, height: f64) -> PyClassInitializer<Self> {
            PyClassInitializer::from(PyExternalShape(ExternalShape::Rectangle { width, height }))
                .add_subclass(Self)
        }

        #[classattr]
        fn __match_args__(py: Python<'_>) -> PyResult<Bound<'_, PyTuple>> {
            PyTuple::new(py, ["width", "height"])
        }

        #[getter]
        fn width(self_: PyRef<'_, Self>) -> f64 {
            match self_.as_ref().0 {
                ExternalShape::Rectangle { width, .. } => width,
                _ => unreachable!("wrong variant"),
            }
        }

        #[getter]
        fn height(self_: PyRef<'_, Self>) -> f64 {
            match self_.as_ref().0 {
                ExternalShape::Rectangle { height, .. } => height,
                _ => unreachable!("wrong variant"),
            }
        }
    }
}

#[test]
fn external_enum_variant_classes() {
    use external_enum::*;

    Python::attach(|py| {
        let cls = py.get_type::<PyExternalShape>();
        py_run!(
            py,
            cls,
            r#"
            circle = cls.Circle(10.0)
            assert isinstance(circle, cls)
            assert isinstance(circle, cls.Circle)
            assert not isinstance(circle, cls.Rectangle)
            assert circle.radius == 10.0

            square = cls.Rectangle(width=1.0, height=1.0)
            assert isinstance(square, cls.Rectangle)
            assert not isinstance(square, cls.Circle)
            assert square.width == 1.0
            assert square.height == 1.0
        "#
        );
    });
}

#[test]
#[cfg(Py_3_10)]
fn external_enum_variant_classes_pattern_match() {
    use external_enum::*;

    Python::attach(|py| {
        let cls = py.get_type::<PyExternalShape>();
        py_run!(
            py,
            cls,
            r#"
            def describe(cls, shape):
                match shape:
                    case cls.Circle(radius):
                        return ('circle', radius)
                    case cls.Rectangle(width, height):
                        return ('rectangle', width, height)

            assert describe(cls, cls.Circle(10.0)) == ('circle', 10.0)
            assert describe(cls, cls.Rectangle(width=1.0, height=2.0)) == ('rectangle', 1.0, 2.0)
        "#
        );
    });
}

#[test]
fn external_enum_variant_classes_from_rust() {
    use external_enum::*;

    Python::attach(|py| {
        let circle =
            PyExternalShape::from_shape(ExternalShape::Circle { radius: 2.0 }, py).unwrap();
        let circle_cls = py.get_type::<PyExternalShapeCircle>();
        assert!(circle.bind(py).is_instance(&circle_cls).unwrap());
        // The base class is still reachable through the variant class
        let base: PyRef<'_, PyExternalShape> =
            circle.bind(py).cast::<PyExternalShape>().unwrap().borrow();
        assert_eq!(base.0, ExternalShape::Circle { radius: 2.0 });
    });
}
