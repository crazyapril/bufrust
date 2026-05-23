use crate::descriptor::Descriptor;
use crate::error::{BufrError, Result};
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufrMessage {
    #[pyo3(get)]
    pub total_length: usize,
    #[pyo3(get)]
    pub edition: u8,
    #[pyo3(get)]
    pub section1_length: usize,
    #[pyo3(get)]
    pub master_table_number: u8,
    #[pyo3(get)]
    pub originating_centre: u16,
    #[pyo3(get)]
    pub originating_subcentre: u16,
    #[pyo3(get)]
    pub update_sequence_number: u8,
    #[pyo3(get)]
    pub local_section_present: bool,
    #[pyo3(get)]
    pub data_category: u8,
    #[pyo3(get)]
    pub international_data_subcategory: u8,
    #[pyo3(get)]
    pub local_data_subcategory: u8,
    #[pyo3(get)]
    pub master_tables_version_number: u8,
    #[pyo3(get)]
    pub local_tables_version_number: u8,
    #[pyo3(get)]
    pub typical_year: u16,
    #[pyo3(get)]
    pub typical_month: u8,
    #[pyo3(get)]
    pub typical_day: u8,
    #[pyo3(get)]
    pub typical_hour: u8,
    #[pyo3(get)]
    pub typical_minute: u8,
    #[pyo3(get)]
    pub typical_second: u8,
    #[pyo3(get)]
    pub section2_length: usize,
    #[pyo3(get)]
    pub section3_length: usize,
    #[pyo3(get)]
    pub number_of_subsets: u16,
    #[pyo3(get)]
    pub observed_data: bool,
    #[pyo3(get)]
    pub compressed_data: bool,
    #[pyo3(get)]
    pub unexpanded_descriptors: Vec<u32>,
    #[pyo3(get)]
    pub section4_length: usize,
    #[pyo3(get)]
    pub section4_data_offset: usize,
    #[pyo3(get)]
    pub section4_data_length: usize,
}

#[pymethods]
impl BufrMessage {
    fn __repr__(&self) -> String {
        format!(
            "BufrMessage(edition={}, length={}, subsets={}, descriptors={:?})",
            self.edition, self.total_length, self.number_of_subsets, self.unexpanded_descriptors
        )
    }
}

pub fn parse_message(data: &[u8]) -> Result<BufrMessage> {
    if data.len() < 8 {
        return Err(BufrError::TooShort);
    }
    if &data[0..4] != b"BUFR" {
        return Err(BufrError::MissingMagic);
    }

    let total_length = read_u24(data, 4)?;
    let edition = data[7];
    if edition != 4 {
        return Err(BufrError::UnsupportedEdition(edition));
    }
    if total_length != data.len() {
        return Err(BufrError::LengthMismatch {
            declared: total_length,
            actual: data.len(),
        });
    }

    let section1_offset = 8;
    let section1_length = read_section_length(data, section1_offset, 1)?;
    require(data, section1_offset, section1_length, 1)?;

    let flags = data[section1_offset + 9];
    let local_section_present = flags != 0;
    let typical_year = read_u16(data, section1_offset + 15)?;

    let section2_offset = section1_offset + section1_length;
    let section2_length = if local_section_present {
        let len = read_section_length(data, section2_offset, 2)?;
        require(data, section2_offset, len, 2)?;
        len
    } else {
        0
    };

    let section3_offset = section2_offset + section2_length;
    let section3_length = read_section_length(data, section3_offset, 3)?;
    require(data, section3_offset, section3_length, 3)?;
    if section3_length < 7 {
        return Err(BufrError::TruncatedSection {
            section: 3,
            offset: section3_offset,
        });
    }

    let number_of_subsets = read_u16(data, section3_offset + 4)?;
    let section3_flags = data[section3_offset + 6];
    let descriptor_bytes = section3_length - 7;
    if descriptor_bytes % 2 != 0 {
        return Err(BufrError::BadDescriptorLength);
    }
    let mut unexpanded_descriptors = Vec::with_capacity(descriptor_bytes / 2);
    let mut pos = section3_offset + 7;
    while pos < section3_offset + section3_length {
        let descriptor = Descriptor::from_packed([data[pos], data[pos + 1]]);
        unexpanded_descriptors.push(descriptor.code());
        pos += 2;
    }

    let section4_offset = section3_offset + section3_length;
    let section4_length = read_section_length(data, section4_offset, 4)?;
    require(data, section4_offset, section4_length, 4)?;
    let section4_data_offset = section4_offset + 4;
    let section4_data_length = section4_length.saturating_sub(4);

    let section5_offset = section4_offset + section4_length;
    require(data, section5_offset, 4, 5)?;
    if &data[section5_offset..section5_offset + 4] != b"7777" {
        return Err(BufrError::BadTerminator);
    }

    Ok(BufrMessage {
        total_length,
        edition,
        section1_length,
        master_table_number: data[section1_offset + 3],
        originating_centre: read_u16(data, section1_offset + 4)?,
        originating_subcentre: read_u16(data, section1_offset + 6)?,
        update_sequence_number: data[section1_offset + 8],
        local_section_present,
        data_category: data[section1_offset + 10],
        international_data_subcategory: data[section1_offset + 11],
        local_data_subcategory: data[section1_offset + 12],
        master_tables_version_number: data[section1_offset + 13],
        local_tables_version_number: data[section1_offset + 14],
        typical_year,
        typical_month: data[section1_offset + 17],
        typical_day: data[section1_offset + 18],
        typical_hour: data[section1_offset + 19],
        typical_minute: data[section1_offset + 20],
        typical_second: data[section1_offset + 21],
        section2_length,
        section3_length,
        number_of_subsets,
        observed_data: section3_flags & (1 << 7) != 0,
        compressed_data: section3_flags & (1 << 6) != 0,
        unexpanded_descriptors,
        section4_length,
        section4_data_offset,
        section4_data_length,
    })
}

pub fn split_messages(data: &[u8]) -> Result<Vec<&[u8]>> {
    let mut messages = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        let Some(relative) = data[offset..]
            .windows(4)
            .position(|window| window == b"BUFR")
        else {
            break;
        };
        let start = offset + relative;
        if start + 8 > data.len() {
            return Err(BufrError::TooShort);
        }
        let total_length = read_u24(data, start + 4)?;
        require(data, start, total_length, 0)?;
        messages.push(&data[start..start + total_length]);
        offset = start + total_length;
    }
    Ok(messages)
}

pub fn parse_messages(data: &[u8]) -> Result<Vec<BufrMessage>> {
    split_messages(data)?
        .into_iter()
        .map(parse_message)
        .collect()
}

fn read_section_length(data: &[u8], offset: usize, section: u8) -> Result<usize> {
    require(data, offset, 3, section)?;
    read_u24(data, offset)
}

fn read_u24(data: &[u8], offset: usize) -> Result<usize> {
    require(data, offset, 3, 0)?;
    Ok(((data[offset] as usize) << 16)
        | ((data[offset + 1] as usize) << 8)
        | data[offset + 2] as usize)
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    require(data, offset, 2, 0)?;
    Ok(u16::from_be_bytes([data[offset], data[offset + 1]]))
}

fn require(data: &[u8], offset: usize, len: usize, section: u8) -> Result<()> {
    if offset.checked_add(len).is_some_and(|end| end <= data.len()) {
        Ok(())
    } else {
        Err(BufrError::TruncatedSection { section, offset })
    }
}
