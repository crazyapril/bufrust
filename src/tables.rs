use crate::descriptor::Descriptor;
use crate::error::{BufrError, Result};
use crate::message::BufrMessage;
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementDefinition {
    #[pyo3(get)]
    pub code: u32,
    #[pyo3(get)]
    pub abbreviation: String,
    #[pyo3(get)]
    pub element_type: String,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub unit: String,
    #[pyo3(get)]
    pub scale: i32,
    #[pyo3(get)]
    pub reference: i64,
    #[pyo3(get)]
    pub width: u16,
}

#[pymethods]
impl ElementDefinition {
    fn __repr__(&self) -> String {
        format!("ElementDefinition({:06}, name={:?})", self.code, self.name)
    }
}

#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceDefinition {
    #[pyo3(get)]
    pub code: u32,
    #[pyo3(get)]
    pub members: Vec<u32>,
}

#[pymethods]
impl SequenceDefinition {
    fn __repr__(&self) -> String {
        format!(
            "SequenceDefinition({:06}, members={:?})",
            self.code, self.members
        )
    }
}

#[pyclass]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TableSet {
    elements: HashMap<u32, ElementDefinition>,
    sequences: HashMap<u32, SequenceDefinition>,
}

impl TableSet {
    pub fn element(&self, code: u32) -> Option<&ElementDefinition> {
        self.elements.get(&code)
    }

    pub fn sequence(&self, code: u32) -> Option<&SequenceDefinition> {
        self.sequences.get(&code)
    }

    pub fn insert_element(&mut self, element: ElementDefinition) {
        self.elements.insert(element.code, element);
    }

    pub fn insert_sequence(&mut self, sequence: SequenceDefinition) {
        self.sequences.insert(sequence.code, sequence);
    }

    pub fn from_eccodes_dir(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut tables = Self::default();
        tables.load_eccodes_element_table(path.join("element.table"))?;
        tables.load_eccodes_sequence_def(path.join("sequence.def"))?;
        Ok(tables)
    }

    pub fn from_eccodes_definitions(root: impl AsRef<Path>, message: &BufrMessage) -> Result<Self> {
        let root = root.as_ref();
        let master_dir = resolve_eccodes_table_dir(
            root.join("bufr")
                .join("tables")
                .join(message.master_table_number.to_string())
                .join("wmo")
                .join(message.master_tables_version_number.to_string()),
        )?;
        let mut tables = Self::from_eccodes_dir(master_dir)?;
        if message.local_tables_version_number != 0 {
            let local_dir = local_table_dir(root, message);
            if local_dir.exists() {
                let element = local_dir.join("element.table");
                if element.exists() {
                    tables.load_eccodes_element_table(element)?;
                }
                let sequence = local_dir.join("sequence.def");
                if sequence.exists() {
                    tables.load_eccodes_sequence_def(sequence)?;
                }
            }
        }
        Ok(tables)
    }

    pub fn builtin_definitions_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("python")
            .join("bufrust")
            .join("definitions")
    }

    pub fn from_builtin_definitions(message: &BufrMessage) -> Result<Self> {
        Self::from_eccodes_definitions(Self::builtin_definitions_dir(), message)
    }

    pub fn from_bufr4_45_dir(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut tables = Self::default();
        for file in matching_files(path, "BUFRCREX_TableB_en_", ".csv")? {
            tables.load_bufr4_45_table_b(file)?;
        }
        for file in matching_files(path, "BUFR_TableD_en_", ".csv")? {
            tables.load_bufr4_45_table_d(file)?;
        }
        Ok(tables)
    }

    pub fn load_eccodes_element_table(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b'|')
            .comment(Some(b'#'))
            .has_headers(false)
            .from_path(path)
            .map_err(|err| BufrError::Table(err.to_string()))?;

        for row in reader.records() {
            let row = row.map_err(|err| BufrError::Table(err.to_string()))?;
            if row.len() < 8 {
                continue;
            }
            let code = parse_code(&row[0])?;
            self.insert_element(ElementDefinition {
                code,
                abbreviation: row[1].to_string(),
                element_type: row[2].to_string(),
                name: row[3].to_string(),
                unit: row[4].to_string(),
                scale: parse_i32(&row[5])?,
                reference: parse_i64(&row[6])?,
                width: parse_u16(&row[7])?,
            });
        }
        Ok(())
    }

    pub fn load_eccodes_sequence_def(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let text = std::fs::read_to_string(path)?;
        let mut pending = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if pending.is_empty() {
                pending.push_str(line);
            } else {
                pending.push(' ');
                pending.push_str(line);
            }
            if !pending.contains(']') {
                continue;
            };
            let entry = std::mem::take(&mut pending);
            let Some((lhs, rhs)) = entry.split_once('=') else {
                continue;
            };
            let code = parse_code(lhs.trim().trim_matches('"'))?;
            let start = rhs
                .find('[')
                .ok_or_else(|| BufrError::Table(entry.clone()))?;
            let end = rhs
                .find(']')
                .ok_or_else(|| BufrError::Table(entry.clone()))?;
            let members = rhs[start + 1..end]
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(parse_code)
                .collect::<Result<Vec<_>>>()?;
            self.insert_sequence(SequenceDefinition { code, members });
        }
        Ok(())
    }

    pub fn load_bufr4_45_table_b(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let mut reader =
            csv::Reader::from_path(path).map_err(|err| BufrError::Table(err.to_string()))?;
        for row in reader.records() {
            let row = row.map_err(|err| BufrError::Table(err.to_string()))?;
            if row.len() < 8 {
                continue;
            }
            let code = parse_code(&row[2])?;
            self.insert_element(ElementDefinition {
                code,
                abbreviation: String::new(),
                element_type: unit_to_type(&row[4]).to_string(),
                name: row[3].to_string(),
                unit: row[4].to_string(),
                scale: parse_i32(&row[5])?,
                reference: parse_i64(&row[6])?,
                width: parse_u16(&row[7])?,
            });
        }
        Ok(())
    }

    pub fn load_bufr4_45_table_d(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let mut reader =
            csv::Reader::from_path(path).map_err(|err| BufrError::Table(err.to_string()))?;
        let mut sequences: HashMap<u32, Vec<u32>> = HashMap::new();
        for row in reader.records() {
            let row = row.map_err(|err| BufrError::Table(err.to_string()))?;
            if row.len() < 6 {
                continue;
            }
            let sequence_code = parse_code(&row[2])?;
            let member_code = parse_code(&row[5])?;
            sequences
                .entry(sequence_code)
                .or_default()
                .push(member_code);
        }
        for (code, members) in sequences {
            self.insert_sequence(SequenceDefinition { code, members });
        }
        Ok(())
    }

    pub fn expand(&self, descriptors: &[u32]) -> Result<Vec<u32>> {
        self.expand_codes(descriptors)
    }

    pub fn expand_codes(&self, descriptors: &[u32]) -> Result<Vec<u32>> {
        let mut out = Vec::new();
        let mut coding = CodingState::default();
        self.expand_into(descriptors, &mut out, &mut coding, 0)?;
        Ok(out)
    }

    fn expand_into(
        &self,
        descriptors: &[u32],
        out: &mut Vec<u32>,
        coding: &mut CodingState,
        depth: usize,
    ) -> Result<usize> {
        if depth > 64 {
            return Err(BufrError::Table(
                "sequence expansion exceeded recursion depth".into(),
            ));
        }

        let start_len = out.len();
        let mut i = 0;
        while i < descriptors.len() {
            let code = descriptors[i];
            let descriptor = Descriptor::from_code(code);
            match descriptor.f {
                0 => {
                    if coding.associated_field_width > 0 && descriptor.x != 31 {
                        out.push(999999);
                    }
                    out.push(code);
                    if coding.local_descriptor_width.is_some() {
                        coding.local_descriptor_width = None;
                    }
                }
                1 => {
                    let count = descriptor.x as usize;
                    if count == 0 || i + count >= descriptors.len() {
                        return Err(BufrError::UnsupportedReplication(code));
                    }
                    if descriptor.y == 0 {
                        if i + count + 1 >= descriptors.len() {
                            return Err(BufrError::DelayedReplication(code));
                        }
                        let group = &descriptors[i + 1..i + 2 + count];
                        self.expand_into(group, out, coding, depth + 1)?;
                        i += count + 1;
                    } else {
                        let group = &descriptors[i + 1..i + 1 + count];
                        for _ in 0..descriptor.y {
                            self.expand_into(group, out, coding, depth + 1)?;
                        }
                        i += count;
                    }
                }
                2 => match descriptor.x {
                    1 => {
                        coding.extra_width = if descriptor.y == 0 {
                            0
                        } else {
                            descriptor.y as i32 - 128
                        }
                    }
                    2 => {
                        coding.extra_scale = if descriptor.y == 0 {
                            0
                        } else {
                            descriptor.y as i32 - 128
                        }
                    }
                    4 => coding.associated_field_width = descriptor.y,
                    6 => coding.local_descriptor_width = Some(descriptor.y),
                    7 => {
                        if descriptor.y == 0 {
                            coding.extra_width = 0;
                            coding.extra_scale = 0;
                        } else {
                            coding.extra_scale = descriptor.y as i32;
                            coding.extra_width = ((10 * descriptor.y as i32) + 2) / 3;
                        }
                    }
                    8 => coding.new_string_width = Some(descriptor.y as u16 * 8),
                    _ => out.push(code),
                },
                3 => {
                    let sequence = self
                        .sequence(code)
                        .ok_or(BufrError::MissingSequence(code))?;
                    self.expand_into(&sequence.members, out, coding, depth + 1)?;
                }
                _ => return Err(BufrError::Table(format!("invalid descriptor {code:06}"))),
            }
            i += 1;
        }
        Ok(out.len() - start_len)
    }
}

#[derive(Debug, Clone, Default)]
struct CodingState {
    extra_width: i32,
    extra_scale: i32,
    associated_field_width: u8,
    local_descriptor_width: Option<u8>,
    new_string_width: Option<u16>,
}

#[pymethods]
impl TableSet {
    #[new]
    fn py_new() -> Self {
        Self::default()
    }

    #[staticmethod]
    fn from_eccodes(path: &str) -> PyResult<Self> {
        Self::from_eccodes_dir(path).map_err(to_py_err)
    }

    #[staticmethod]
    fn from_bufr4_45(path: &str) -> PyResult<Self> {
        Self::from_bufr4_45_dir(path).map_err(to_py_err)
    }

    #[staticmethod]
    fn from_definitions(path: &str, message: &BufrMessage) -> PyResult<Self> {
        Self::from_eccodes_definitions(path, message).map_err(to_py_err)
    }

    #[getter]
    fn element_count(&self) -> usize {
        self.elements.len()
    }

    #[getter]
    fn sequence_count(&self) -> usize {
        self.sequences.len()
    }

    fn get_element(&self, code: u32) -> Option<ElementDefinition> {
        self.element(code).cloned()
    }

    fn get_sequence(&self, code: u32) -> Option<SequenceDefinition> {
        self.sequence(code).cloned()
    }

    #[pyo3(name = "expand")]
    fn py_expand(&self, descriptors: Vec<u32>) -> PyResult<Vec<u32>> {
        TableSet::expand(self, &descriptors).map_err(to_py_err)
    }

    fn __repr__(&self) -> String {
        format!(
            "TableSet(elements={}, sequences={})",
            self.elements.len(),
            self.sequences.len()
        )
    }
}

pub fn to_py_err(err: BufrError) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(err.to_string())
}

fn parse_code(text: &str) -> Result<u32> {
    text.trim()
        .trim_matches('"')
        .parse::<u32>()
        .map_err(|err| BufrError::Table(format!("bad descriptor code {text:?}: {err}")))
}

fn parse_i32(text: &str) -> Result<i32> {
    text.parse::<i32>()
        .map_err(|err| BufrError::Table(format!("bad i32 {text:?}: {err}")))
}

fn parse_i64(text: &str) -> Result<i64> {
    text.parse::<i64>()
        .map_err(|err| BufrError::Table(format!("bad i64 {text:?}: {err}")))
}

fn parse_u16(text: &str) -> Result<u16> {
    text.parse::<u16>()
        .map_err(|err| BufrError::Table(format!("bad u16 {text:?}: {err}")))
}

fn matching_files(path: &Path, prefix: &str, suffix: &str) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name.starts_with(prefix) && file_name.ends_with(suffix) {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

fn resolve_eccodes_table_dir(path: PathBuf) -> Result<PathBuf> {
    if path.is_dir() {
        return Ok(path);
    }
    if path.is_file() {
        let alias = std::fs::read_to_string(&path)?;
        let alias = alias.trim();
        if alias.is_empty()
            || alias.contains(std::path::MAIN_SEPARATOR)
            || alias.contains('/')
            || alias.contains('\\')
        {
            return Err(BufrError::Table(format!(
                "invalid ecCodes table alias {} -> {alias:?}",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            return resolve_eccodes_table_dir(parent.join(alias));
        }
    }
    Err(BufrError::Table(format!(
        "ecCodes table directory does not exist: {}",
        path.display()
    )))
}

fn unit_to_type(unit: &str) -> &'static str {
    match unit {
        "CCITT IA5" => "string",
        "Code table" => "code",
        "Flag table" => "flags",
        _ => "long",
    }
}

fn local_table_dir(root: &Path, message: &BufrMessage) -> PathBuf {
    let local_version = if message.master_tables_version_number == 19 {
        format!(
            "{}-{}",
            message.master_tables_version_number, message.local_tables_version_number
        )
    } else {
        message.local_tables_version_number.to_string()
    };
    root.join("bufr")
        .join("tables")
        .join(message.master_table_number.to_string())
        .join("local")
        .join(local_version)
        .join(message.originating_centre.to_string())
        .join(message.originating_subcentre.to_string())
}
