mod bitstream;
mod data;
mod descriptor;
mod error;
mod message;
mod tables;

pub use data::{decode_uncompressed_values, DecodedValue};
pub use descriptor::Descriptor;
pub use error::{BufrError, Result};
pub use message::{parse_message, parse_messages, split_messages, BufrMessage};
pub use tables::{ElementDefinition, SequenceDefinition, TableSet};

use pyo3::prelude::*;

#[pyfunction]
fn parse_bytes(data: &[u8]) -> PyResult<BufrMessage> {
    parse_message(data).map_err(tables::to_py_err)
}

#[pyfunction]
fn parse_file(path: &str) -> PyResult<BufrMessage> {
    let data =
        std::fs::read(path).map_err(|err| pyo3::exceptions::PyOSError::new_err(err.to_string()))?;
    parse_message(&data).map_err(tables::to_py_err)
}

#[pyfunction]
fn parse_all_bytes(data: &[u8]) -> PyResult<Vec<BufrMessage>> {
    parse_messages(data).map_err(tables::to_py_err)
}

#[pyfunction]
fn parse_all_file(path: &str) -> PyResult<Vec<BufrMessage>> {
    let data =
        std::fs::read(path).map_err(|err| pyo3::exceptions::PyOSError::new_err(err.to_string()))?;
    parse_messages(&data).map_err(tables::to_py_err)
}

#[pyfunction]
fn expand_descriptors(table_dir: &str, descriptors: Vec<u32>) -> PyResult<Vec<u32>> {
    let tables = TableSet::from_eccodes_dir(table_dir).map_err(tables::to_py_err)?;
    tables.expand(&descriptors).map_err(tables::to_py_err)
}

#[pyfunction]
fn decode_values(data: &[u8], table_dir: &str) -> PyResult<Vec<DecodedValue>> {
    let data = first_message_data(data).map_err(tables::to_py_err)?;
    let message = parse_message(data).map_err(tables::to_py_err)?;
    let tables = TableSet::from_eccodes_dir(table_dir).map_err(tables::to_py_err)?;
    let section4 = &data
        [message.section4_data_offset..message.section4_data_offset + message.section4_data_length];
    decode_uncompressed_values(&message, &tables, section4).map_err(tables::to_py_err)
}

#[pyfunction]
fn decode_values_with_tables(data: &[u8], tables: &TableSet) -> PyResult<Vec<DecodedValue>> {
    let data = first_message_data(data).map_err(tables::to_py_err)?;
    let message = parse_message(data).map_err(tables::to_py_err)?;
    let section4 = &data
        [message.section4_data_offset..message.section4_data_offset + message.section4_data_length];
    decode_uncompressed_values(&message, tables, section4).map_err(tables::to_py_err)
}

#[pyfunction]
fn decode_values_with_definitions(
    data: &[u8],
    definitions_root: &str,
) -> PyResult<Vec<DecodedValue>> {
    let data = first_message_data(data).map_err(tables::to_py_err)?;
    let message = parse_message(data).map_err(tables::to_py_err)?;
    let tables = TableSet::from_eccodes_definitions(definitions_root, &message)
        .map_err(tables::to_py_err)?;
    let section4 = &data
        [message.section4_data_offset..message.section4_data_offset + message.section4_data_length];
    decode_uncompressed_values(&message, &tables, section4).map_err(tables::to_py_err)
}

pub fn decode_values_with_builtin_tables(data: &[u8]) -> Result<Vec<DecodedValue>> {
    let data = first_message_data(data)?;
    let message = parse_message(data)?;
    let tables = TableSet::from_builtin_definitions(&message)?;
    let section4 = &data
        [message.section4_data_offset..message.section4_data_offset + message.section4_data_length];
    decode_uncompressed_values(&message, &tables, section4)
}

fn first_message_data(data: &[u8]) -> Result<&[u8]> {
    split_messages(data)?
        .into_iter()
        .next()
        .ok_or(BufrError::MissingMagic)
}

#[pymodule]
fn _bufrust(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<BufrMessage>()?;
    m.add_class::<DecodedValue>()?;
    m.add_class::<Descriptor>()?;
    m.add_class::<ElementDefinition>()?;
    m.add_class::<SequenceDefinition>()?;
    m.add_class::<TableSet>()?;
    m.add_function(wrap_pyfunction!(parse_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(parse_file, m)?)?;
    m.add_function(wrap_pyfunction!(parse_all_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(parse_all_file, m)?)?;
    m.add_function(wrap_pyfunction!(expand_descriptors, m)?)?;
    m.add_function(wrap_pyfunction!(decode_values, m)?)?;
    m.add_function(wrap_pyfunction!(decode_values_with_tables, m)?)?;
    m.add_function(wrap_pyfunction!(decode_values_with_definitions, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_round_trip() {
        let descriptor = Descriptor::from_packed([0b1100_0010, 0b0000_1000]);
        assert_eq!(descriptor.code(), 302008);
        assert_eq!(Descriptor::from_code(302008), descriptor);
    }

    #[test]
    fn parses_minimal_edition4_message() {
        let message = minimal_message();
        let parsed = parse_message(&message).unwrap();
        assert_eq!(parsed.edition, 4);
        assert_eq!(parsed.total_length, message.len());
        assert_eq!(parsed.master_tables_version_number, 42);
        assert_eq!(parsed.number_of_subsets, 1);
        assert!(parsed.observed_data);
        assert_eq!(parsed.unexpanded_descriptors, vec![307080]);
    }

    #[test]
    fn expands_fixed_replication_and_sequences() {
        let mut tables = TableSet::default();
        tables.insert_sequence(SequenceDefinition {
            code: 300001,
            members: vec![1001, 1002],
        });
        let expanded = tables.expand(&[101002, 300001]).unwrap();
        assert_eq!(expanded, vec![1001, 1002, 1001, 1002]);
    }

    #[test]
    fn decodes_uncompressed_numeric_value() {
        let mut message = minimal_message();
        message[6] = 49;
        message[37] = 0b0000_0100;
        message[38] = 0b0000_0001;
        message[41] = 6;
        message.insert(43, 0x7e);
        message.insert(44, 0xa0);
        let parsed = parse_message(&message).unwrap();

        let mut tables = TableSet::default();
        tables.insert_element(ElementDefinition {
            code: 4001,
            abbreviation: "year".into(),
            element_type: "long".into(),
            name: "Year".into(),
            unit: "a".into(),
            scale: 0,
            reference: 0,
            width: 12,
        });

        let data = &message[parsed.section4_data_offset
            ..parsed.section4_data_offset + parsed.section4_data_length];
        let values = decode_uncompressed_values(&parsed, &tables, data).unwrap();
        assert_eq!(values[0].descriptor, 4001);
        assert_eq!(values[0].raw_value, Some(2026.0));
    }

    #[test]
    fn splits_concatenated_messages() {
        let one = minimal_message();
        let mut two = Vec::new();
        two.extend_from_slice(&one);
        two.extend_from_slice(&one);
        let messages = parse_messages(&two).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].unexpanded_descriptors, vec![307080]);
        assert_eq!(messages[1].unexpanded_descriptors, vec![307080]);
    }

    fn minimal_message() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"BUFR");
        data.extend_from_slice(&[0, 0, 47]);
        data.push(4);

        data.extend_from_slice(&[0, 0, 22]);
        data.push(0);
        data.extend_from_slice(&98u16.to_be_bytes());
        data.extend_from_slice(&0u16.to_be_bytes());
        data.push(0);
        data.push(0);
        data.push(0);
        data.push(0);
        data.push(0);
        data.push(42);
        data.push(0);
        data.extend_from_slice(&2026u16.to_be_bytes());
        data.extend_from_slice(&[5, 22, 12, 0, 0]);

        data.extend_from_slice(&[0, 0, 9]);
        data.push(0);
        data.extend_from_slice(&1u16.to_be_bytes());
        data.push(0b1000_0000);
        data.extend_from_slice(&[0b1100_0111, 0b0101_0000]);

        data.extend_from_slice(&[0, 0, 4, 0]);
        data.extend_from_slice(b"7777");
        data
    }
}
