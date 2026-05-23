use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

#[pyclass]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Descriptor {
    #[pyo3(get)]
    pub f: u8,
    #[pyo3(get)]
    pub x: u8,
    #[pyo3(get)]
    pub y: u8,
}

impl Descriptor {
    pub fn new(f: u8, x: u8, y: u8) -> Self {
        Self { f, x, y }
    }

    pub fn from_code(code: u32) -> Self {
        let f = (code / 100_000) as u8;
        let rem = code % 100_000;
        let x = (rem / 1_000) as u8;
        let y = (rem % 1_000) as u8;
        Self { f, x, y }
    }

    pub fn code(self) -> u32 {
        self.f as u32 * 100_000 + self.x as u32 * 1_000 + self.y as u32
    }

    pub fn from_packed(bytes: [u8; 2]) -> Self {
        let raw = u16::from_be_bytes(bytes);
        let f = ((raw >> 14) & 0b11) as u8;
        let x = ((raw >> 8) & 0b11_1111) as u8;
        let y = (raw & 0xff) as u8;
        Self { f, x, y }
    }
}

#[pymethods]
impl Descriptor {
    #[new]
    fn py_new(f: u8, x: u8, y: u8) -> Self {
        Self::new(f, x, y)
    }

    #[staticmethod]
    fn from_fxy(code: u32) -> Self {
        Self::from_code(code)
    }

    #[getter]
    fn fxy(&self) -> u32 {
        self.code()
    }

    fn __repr__(&self) -> String {
        format!("Descriptor({:06})", self.code())
    }
}
