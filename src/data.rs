use crate::bitstream::BitReader;
use crate::descriptor::Descriptor;
use crate::error::{BufrError, Result};
use crate::message::BufrMessage;
use crate::tables::{ElementDefinition, TableSet};
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedValue {
    #[pyo3(get)]
    pub descriptor: u32,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub value: Option<f64>,
    #[pyo3(get)]
    pub raw: Option<u64>,
    #[pyo3(get)]
    pub text: Option<String>,
}

#[pymethods]
impl DecodedValue {
    fn __repr__(&self) -> String {
        if let Some(text) = &self.text {
            format!("DecodedValue({:06}, {:?})", self.descriptor, text)
        } else {
            format!("DecodedValue({:06}, {:?})", self.descriptor, self.value)
        }
    }
}

pub fn decode_uncompressed_values(
    message: &BufrMessage,
    tables: &TableSet,
    data: &[u8],
) -> Result<Vec<DecodedValue>> {
    let mut reader = BitReader::new(data);
    if message.compressed_data {
        return decode_compressed_values(message, tables, &mut reader);
    }

    let mut values = Vec::new();
    let mut string_value_count = 0usize;

    for _subset in 0..message.number_of_subsets {
        let mut coding = ValueCodingState::default();
        coding.string_value_count = string_value_count;
        decode_descriptor_values_inner(
            &message.unexpanded_descriptors,
            tables,
            &mut reader,
            &mut coding,
            &mut values,
            0,
        )?;
        string_value_count = coding.string_value_count;
    }

    Ok(values)
}

fn decode_compressed_values(
    message: &BufrMessage,
    tables: &TableSet,
    reader: &mut BitReader<'_>,
) -> Result<Vec<DecodedValue>> {
    let subset_count = message.number_of_subsets as usize;
    let mut columns: Vec<Vec<DecodedValue>> = Vec::new();
    let mut coding = ValueCodingState::default();

    decode_compressed_descriptors_inner(
        &message.unexpanded_descriptors,
        tables,
        reader,
        &mut coding,
        &mut columns,
        subset_count,
        0,
    )?;

    let mut values = Vec::new();
    for subset in 0..subset_count {
        for column in &columns {
            if let Some(value) = column.get(subset) {
                values.push(value.clone());
            }
        }
    }
    Ok(values)
}

fn decode_compressed_descriptors_inner(
    descriptors: &[u32],
    tables: &TableSet,
    reader: &mut BitReader<'_>,
    coding: &mut ValueCodingState,
    columns: &mut Vec<Vec<DecodedValue>>,
    subset_count: usize,
    depth: usize,
) -> Result<usize> {
    if depth > 64 {
        return Err(BufrError::Table(
            "compressed descriptor decode exceeded recursion depth".into(),
        ));
    }

    let mut i = 0;
    while i < descriptors.len() {
        let code = descriptors[i];
        let descriptor = Descriptor::from_code(code);
        match descriptor.f {
            0 => {
                if coding.associated_field_width > 0 && descriptor.x != 31 {
                    columns.push(read_compressed_numeric_column(
                        999999,
                        "associatedField",
                        coding.associated_field_width as u16,
                        0,
                        0,
                        false,
                        subset_count,
                        reader,
                    )?);
                }
                let element = tables
                    .element(code)
                    .ok_or(BufrError::MissingElement(code))?;
                let column = read_compressed_element_column(element, coding, subset_count, reader)?;
                if code == 31031 && coding.collecting_bitmap {
                    let bit = column.first().and_then(|value| value.value).unwrap_or(0.0) as u8;
                    coding.bitmap.push(bit);
                } else {
                    coding.history.push(element.clone());
                }
                columns.push(column);
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
                    let factor_code = descriptors[i + 1];
                    let factor_element = tables
                        .element(factor_code)
                        .ok_or(BufrError::MissingElement(factor_code))?;
                    let factor_column = read_compressed_element_column(
                        factor_element,
                        coding,
                        subset_count,
                        reader,
                    )?;
                    let factor = constant_replication_factor(&factor_column)?;
                    columns.push(factor_column);
                    coding.history.push(factor_element.clone());
                    let group = &descriptors[i + 2..i + 2 + count];
                    for _ in 0..factor {
                        decode_compressed_descriptors_inner(
                            group,
                            tables,
                            reader,
                            coding,
                            columns,
                            subset_count,
                            depth + 1,
                        )?;
                    }
                    i += count + 1;
                } else {
                    let group = &descriptors[i + 1..i + 1 + count];
                    for _ in 0..descriptor.y {
                        decode_compressed_descriptors_inner(
                            group,
                            tables,
                            reader,
                            coding,
                            columns,
                            subset_count,
                            depth + 1,
                        )?;
                    }
                    i += count;
                }
            }
            2 => apply_operator_or_column(code, descriptor, reader, coding, columns, subset_count)?,
            3 => {
                let sequence = tables
                    .sequence(code)
                    .ok_or(BufrError::MissingSequence(code))?;
                decode_compressed_descriptors_inner(
                    &sequence.members,
                    tables,
                    reader,
                    coding,
                    columns,
                    subset_count,
                    depth + 1,
                )?;
            }
            _ => return Err(BufrError::Table(format!("invalid descriptor {code:06}"))),
        }
        i += 1;
    }
    Ok(i)
}

fn read_compressed_element_column(
    element: &ElementDefinition,
    coding: &mut ValueCodingState,
    subset_count: usize,
    reader: &mut BitReader<'_>,
) -> Result<Vec<DecodedValue>> {
    let width = if element.unit == "CCITT IA5" {
        coding.new_string_width.unwrap_or(element.width)
    } else {
        coding.local_descriptor_width.unwrap_or_else(|| {
            let adjusted = element.width as i32 + coding.extra_width;
            adjusted.max(0) as u16
        })
    };
    let scale = element.scale + coding.extra_scale;
    let reference = element.reference * coding.reference_factor;
    if element.unit == "CCITT IA5" {
        return read_compressed_string_column(element, width, subset_count, reader, coding);
    }
    read_compressed_numeric_column(
        element.code,
        &element.name,
        width,
        reference,
        scale,
        Descriptor::from_code(element.code).x != 31,
        subset_count,
        reader,
    )
}

fn read_numeric_width(element: &ElementDefinition, coding: &ValueCodingState) -> u16 {
    coding.local_descriptor_width.unwrap_or_else(|| {
        let adjusted = element.width as i32 + coding.extra_width;
        adjusted.max(0) as u16
    })
}

fn read_character_width(element: &ElementDefinition, coding: &ValueCodingState) -> u16 {
    coding.new_string_width.unwrap_or(element.width)
}

fn read_element_width(element: &ElementDefinition, coding: &ValueCodingState) -> u16 {
    if element.unit == "CCITT IA5" {
        read_character_width(element, coding)
    } else {
        read_numeric_width(element, coding)
    }
}

fn scale_and_reference(element: &ElementDefinition, coding: &ValueCodingState) -> (i32, i64) {
    let scale = element.scale + coding.extra_scale;
    let reference = element.reference * coding.reference_factor;
    (scale, reference)
}

fn read_compressed_numeric_column(
    descriptor: u32,
    name: &str,
    width: u16,
    reference: i64,
    scale: i32,
    can_be_missing: bool,
    subset_count: usize,
    reader: &mut BitReader<'_>,
) -> Result<Vec<DecodedValue>> {
    let min_raw = reader.read_u64(width)?;
    let increment_width = reader.read_u64(6)? as u16;
    let min_missing = can_be_missing && width < 64 && min_raw == ((1u64 << width) - 1);
    let mut out = Vec::with_capacity(subset_count);

    for _ in 0..subset_count {
        let increment = if increment_width == 0 {
            Some(0)
        } else {
            let raw = reader.read_u64(increment_width)?;
            if can_be_missing && increment_width < 64 && raw == ((1u64 << increment_width) - 1) {
                None
            } else {
                Some(raw)
            }
        };
        let value = if min_missing {
            None
        } else {
            increment.map(|increment| {
                (min_raw as i128 + increment as i128 + reference as i128) as f64
                    / 10_f64.powi(scale)
            })
        };
        out.push(DecodedValue {
            descriptor,
            name: name.to_string(),
            value,
            raw: Some(min_raw),
            text: None,
        });
    }
    Ok(out)
}

fn read_compressed_string_column(
    element: &ElementDefinition,
    width: u16,
    subset_count: usize,
    reader: &mut BitReader<'_>,
    coding: &mut ValueCodingState,
) -> Result<Vec<DecodedValue>> {
    let min_bytes = reader.read_bytes(width)?;
    let local_width = reader.read_u64(6)? as u16;
    let mut out = Vec::with_capacity(subset_count);
    let string_base = coding.string_column_count * subset_count;
    coding.string_column_count += 1;
    let width_bytes = f64::from(width / 8);

    if local_width > 0 {
        for subset_index in 0..subset_count {
            let bytes = reader.read_bytes(local_width * 8)?;
            let text = decode_ia5_text(&bytes);
            out.push(DecodedValue {
                descriptor: element.code,
                name: element.name.clone(),
                value: Some(((string_base + subset_index + 1) as f64 * 1000.0) + width_bytes),
                raw: None,
                text,
            });
        }
    } else {
        let text = decode_ia5_text(&min_bytes);
        for subset_index in 0..subset_count {
            out.push(DecodedValue {
                descriptor: element.code,
                name: element.name.clone(),
                value: Some(((string_base + subset_index + 1) as f64 * 1000.0) + width_bytes),
                raw: None,
                text: text.clone(),
            });
        }
    }
    Ok(out)
}

fn decode_ia5_text(bytes: &[u8]) -> Option<String> {
    if bytes.iter().all(|byte| *byte == 0xff) {
        None
    } else {
        Some(
            bytes
                .iter()
                .map(|byte| if byte.is_ascii() { *byte as char } else { '?' })
                .collect::<String>()
                .trim_end()
                .to_string(),
        )
    }
}

fn constant_replication_factor(column: &[DecodedValue]) -> Result<usize> {
    let Some(first) = column.first().and_then(|value| value.value) else {
        return Ok(0);
    };
    if column.iter().any(|value| {
        value
            .value
            .map(|value| (value - first).abs() > f64::EPSILON)
            .unwrap_or(true)
    }) {
        return Err(BufrError::Table(
            "compressed delayed replication with varying factors is not implemented".into(),
        ));
    }
    Ok(first.max(0.0) as usize)
}

fn decode_descriptor_values_inner(
    descriptors: &[u32],
    tables: &TableSet,
    reader: &mut BitReader<'_>,
    coding: &mut ValueCodingState,
    values: &mut Vec<DecodedValue>,
    depth: usize,
) -> Result<usize> {
    if depth > 64 {
        return Err(BufrError::Table(
            "descriptor decode exceeded recursion depth".into(),
        ));
    }

    let mut i = 0;
    while i < descriptors.len() {
        let code = descriptors[i];
        let descriptor = Descriptor::from_code(code);
        match descriptor.f {
            0 => {
                read_associated_field(reader, coding, values)?;
                let element = tables
                    .element(code)
                    .ok_or(BufrError::MissingElement(code))?;
                read_element(element, reader, coding, values)?;
                if coding.local_descriptor_width.is_some() {
                    coding.local_descriptor_width = None;
                }
                if code == 31031 && coding.collecting_bitmap {
                    if let Some(value) = values.last().and_then(|value| value.value) {
                        coding.bitmap.push(value as u8);
                    }
                } else {
                    coding.history.push(element.clone());
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
                    let factor_code = descriptors[i + 1];
                    read_associated_field(reader, coding, values)?;
                    let factor_element = tables
                        .element(factor_code)
                        .ok_or(BufrError::MissingElement(factor_code))?;
                    let factor = read_element(factor_element, reader, coding, values)?
                        .unwrap_or(0.0)
                        .max(0.0) as usize;
                    coding.history.push(factor_element.clone());
                    let group = &descriptors[i + 2..i + 2 + count];
                    for _ in 0..factor {
                        decode_descriptor_values_inner(
                            group,
                            tables,
                            reader,
                            coding,
                            values,
                            depth + 1,
                        )?;
                    }
                    i += count + 1;
                } else {
                    let group = &descriptors[i + 1..i + 1 + count];
                    for _ in 0..descriptor.y {
                        decode_descriptor_values_inner(
                            group,
                            tables,
                            reader,
                            coding,
                            values,
                            depth + 1,
                        )?;
                    }
                    i += count;
                }
            }
            2 => apply_operator_or_value(code, descriptor, reader, coding, values)?,
            3 => {
                let sequence = tables
                    .sequence(code)
                    .ok_or(BufrError::MissingSequence(code))?;
                decode_descriptor_values_inner(
                    &sequence.members,
                    tables,
                    reader,
                    coding,
                    values,
                    depth + 1,
                )?;
            }
            _ => return Err(BufrError::Table(format!("invalid descriptor {code:06}"))),
        }
        i += 1;
    }
    Ok(i)
}

fn read_element(
    element: &ElementDefinition,
    reader: &mut BitReader<'_>,
    coding: &mut ValueCodingState,
    values: &mut Vec<DecodedValue>,
) -> Result<Option<f64>> {
    let width = read_element_width(element, coding);
    let (scale, reference) = scale_and_reference(element, coding);

    if element.unit == "CCITT IA5" {
        let bytes = reader.read_bytes(width)?;
        let text = if bytes.iter().all(|byte| *byte == 0xff) {
            None
        } else {
            Some(
                bytes
                    .iter()
                    .map(|byte| if byte.is_ascii() { *byte as char } else { '?' })
                    .collect::<String>()
                    .trim_end()
                    .to_string(),
            )
        };
        coding.string_value_count += 1;
        let string_index = coding.string_value_count;
        values.push(DecodedValue {
            descriptor: element.code,
            name: element.name.clone(),
            value: Some((string_index as f64 * 1000.0) + f64::from(width / 8)),
            raw: None,
            text,
        });
        return Ok(None);
    }

    let raw = reader.read_u64(width)?;
    let can_be_missing = Descriptor::from_code(element.code).x != 31;
    let missing = can_be_missing && width < 64 && raw == ((1u64 << width) - 1);
    let value = if missing {
        None
    } else {
        Some((raw as i128 + reference as i128) as f64 / 10_f64.powi(scale))
    };
    values.push(DecodedValue {
        descriptor: element.code,
        name: element.name.clone(),
        value,
        raw: Some(raw),
        text: None,
    });
    Ok(value)
}

fn read_associated_field(
    reader: &mut BitReader<'_>,
    coding: &ValueCodingState,
    values: &mut Vec<DecodedValue>,
) -> Result<()> {
    if coding.associated_field_width == 0 {
        return Ok(());
    }
    let raw = reader.read_u64(coding.associated_field_width as u16)?;
    values.push(DecodedValue {
        descriptor: 999999,
        name: "associatedField".into(),
        value: Some(raw as f64),
        raw: Some(raw),
        text: None,
    });
    Ok(())
}

fn apply_operator_or_value(
    code: u32,
    descriptor: Descriptor,
    reader: &mut BitReader<'_>,
    coding: &mut ValueCodingState,
    values: &mut Vec<DecodedValue>,
) -> Result<()> {
    if matches!(descriptor.x, 23 | 24 | 25 | 32) && descriptor.y == 255 {
        if let Some(mut element) = coding.next_bitmap_element()? {
            if descriptor.x == 25 {
                element.reference = -2_i64.pow(element.width as u32);
                element.width = element.width.saturating_add(1);
            }
            read_element(&element, reader, coding, values)?;
        }
        return Ok(());
    }
    if operator_emits_zero(descriptor) {
        values.push(DecodedValue {
            descriptor: code,
            name: "operator".into(),
            value: Some(0.0),
            raw: Some(0),
            text: None,
        });
    }
    if descriptor.x == 36 && descriptor.y == 0 {
        coding.collecting_bitmap = true;
        coding.bitmap.clear();
        coding.bitmap_cursor = 0;
        coding.bitmap_history_len = coding.history.len();
    }
    if descriptor.x == 37 && descriptor.y == 0 {
        coding.bitmap_cursor = 0;
    } else if (descriptor.x == 35 && descriptor.y == 0) || (descriptor.x == 37 && descriptor.y != 0)
    {
        coding.bitmap.clear();
        coding.bitmap_cursor = 0;
        coding.bitmap_history_len = 0;
    }
    apply_operator(descriptor, coding);
    Ok(())
}

fn apply_operator_or_column(
    code: u32,
    descriptor: Descriptor,
    reader: &mut BitReader<'_>,
    coding: &mut ValueCodingState,
    columns: &mut Vec<Vec<DecodedValue>>,
    subset_count: usize,
) -> Result<()> {
    if matches!(descriptor.x, 23 | 24 | 25 | 32) && descriptor.y == 255 {
        if let Some(mut element) = coding.next_bitmap_element()? {
            if descriptor.x == 25 {
                element.reference = -2_i64.pow(element.width as u32);
                element.width = element.width.saturating_add(1);
            }
            columns.push(read_compressed_element_column(
                &element,
                coding,
                subset_count,
                reader,
            )?);
        }
        return Ok(());
    }
    if operator_emits_zero(descriptor) {
        columns.push(
            (0..subset_count)
                .map(|_| DecodedValue {
                    descriptor: code,
                    name: "operator".into(),
                    value: Some(0.0),
                    raw: Some(0),
                    text: None,
                })
                .collect(),
        );
    }
    if descriptor.x == 36 && descriptor.y == 0 {
        coding.collecting_bitmap = true;
        coding.bitmap.clear();
        coding.bitmap_cursor = 0;
        coding.bitmap_history_len = coding.history.len();
    }
    if descriptor.x == 37 && descriptor.y == 0 {
        coding.bitmap_cursor = 0;
    } else if (descriptor.x == 35 && descriptor.y == 0) || (descriptor.x == 37 && descriptor.y != 0)
    {
        coding.bitmap.clear();
        coding.bitmap_cursor = 0;
        coding.bitmap_history_len = 0;
    }
    apply_operator(descriptor, coding);
    Ok(())
}

fn operator_emits_zero(descriptor: Descriptor) -> bool {
    matches!(
        descriptor.x,
        26 | 27 | 29 | 30 | 31 | 33 | 34 | 38 | 39 | 40 | 41 | 42
    ) || (matches!(descriptor.x, 22 | 36 | 62) && descriptor.y == 0)
        || (matches!(descriptor.x, 23 | 24 | 25 | 32) && descriptor.y != 255)
        || matches!(descriptor.x, 35 | 37)
}

fn apply_operator(descriptor: Descriptor, coding: &mut ValueCodingState) {
    match descriptor.x {
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
        6 => coding.local_descriptor_width = Some(descriptor.y as u16),
        7 => {
            if descriptor.y == 0 {
                coding.extra_width = 0;
                coding.extra_scale = 0;
                coding.reference_factor = 1;
            } else {
                coding.extra_scale = descriptor.y as i32;
                coding.extra_width = ((10 * descriptor.y as i32) + 2) / 3;
                coding.reference_factor = 10_i64.pow(descriptor.y as u32);
            }
        }
        8 => {
            coding.new_string_width = if descriptor.y == 0 {
                None
            } else {
                Some(descriptor.y as u16 * 8)
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone)]
struct ValueCodingState {
    extra_width: i32,
    extra_scale: i32,
    reference_factor: i64,
    associated_field_width: u8,
    local_descriptor_width: Option<u16>,
    new_string_width: Option<u16>,
    collecting_bitmap: bool,
    bitmap: Vec<u8>,
    bitmap_cursor: usize,
    bitmap_history_len: usize,
    history: Vec<ElementDefinition>,
    string_value_count: usize,
    string_column_count: usize,
}

impl Default for ValueCodingState {
    fn default() -> Self {
        Self {
            extra_width: 0,
            extra_scale: 0,
            reference_factor: 1,
            associated_field_width: 0,
            local_descriptor_width: None,
            new_string_width: None,
            collecting_bitmap: false,
            bitmap: Vec::new(),
            bitmap_cursor: 0,
            bitmap_history_len: 0,
            history: Vec::new(),
            string_value_count: 0,
            string_column_count: 0,
        }
    }
}

impl ValueCodingState {
    fn next_bitmap_element(&mut self) -> Result<Option<ElementDefinition>> {
        if self.bitmap.is_empty() {
            return Ok(None);
        }
        let base = self.bitmap_history_len.saturating_sub(self.bitmap.len());
        while self.bitmap_cursor < self.bitmap.len() && self.bitmap[self.bitmap_cursor] == 1 {
            self.bitmap_cursor += 1;
        }
        if self.bitmap_cursor >= self.bitmap.len() {
            return Ok(None);
        }
        let index = base + self.bitmap_cursor;
        self.bitmap_cursor += 1;
        let element = self
            .history
            .get(index)
            .cloned()
            .ok_or_else(|| {
                BufrError::Table("bitmap reference is outside descriptor history".into())
            })
            .map(Some)?;
        Ok(element)
    }
}
