use bufrust::{decode_uncompressed_values, parse_message, ElementDefinition, TableSet};
use std::fs;
use std::path::PathBuf;

#[test]
fn loads_eccodes_style_tables() {
    let table_dir = test_dir("eccodes-style-tables");
    write_eccodes_tables(&table_dir);

    let tables = TableSet::from_eccodes_dir(&table_dir).unwrap();
    assert_eq!(tables.element(1).unwrap().name, "TABLE A: ENTRY");
    assert_eq!(tables.sequence(300002).unwrap().members, vec![2, 3]);
    assert_eq!(tables.expand(&[300002]).unwrap(), vec![2, 3]);
}

#[test]
fn loads_bufr4_45_style_csv_tables() {
    let table_dir = test_dir("bufr4-45-style-tables");
    fs::create_dir_all(&table_dir).unwrap();
    fs::write(
        table_dir.join("BUFRCREX_TableB_en_test.csv"),
        "FXY,unused,FXY,name,unit,scale,reference,width\n\
         042001,,042001,Dominant swell wave direction of spectral partition,degree true,0,0,9\n",
    )
    .unwrap();
    fs::write(
        table_dir.join("BUFR_TableD_en_test.csv"),
        "FXY,unused,FXY,name,unused,member\n\
         340001,,340001,Example sequence,,001007\n",
    )
    .unwrap();

    let tables = TableSet::from_bufr4_45_dir(&table_dir).unwrap();
    assert_eq!(
        tables.element(42001).unwrap().name,
        "Dominant swell wave direction of spectral partition"
    );
    assert_eq!(tables.sequence(340001).unwrap().members, vec![1007]);
}

#[test]
fn loads_tables_from_message_header() {
    let root = test_dir("definitions-root");
    let table_dir = root
        .join("bufr")
        .join("tables")
        .join("0")
        .join("wmo")
        .join("42");
    write_eccodes_tables(&table_dir);

    let message = minimal_message();
    let parsed = parse_message(&message).unwrap();
    let tables = TableSet::from_eccodes_definitions(&root, &parsed).unwrap();

    assert!(tables.sequence(307080).is_some());
    assert!(tables
        .expand(&parsed.unexpanded_descriptors)
        .unwrap()
        .contains(&4001));
}

#[test]
fn loads_bundled_wmo_tables_from_message_header() {
    let message = minimal_message();
    let parsed = parse_message(&message).unwrap();
    let tables = TableSet::from_builtin_definitions(&parsed).unwrap();

    assert!(tables.sequence(307080).is_some());
    assert!(tables.element(4001).is_some());
}

#[test]
fn decodes_minimal_bufr4_message_with_loaded_tables() {
    let message = minimal_numeric_message();
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

    let section4 = &message
        [parsed.section4_data_offset..parsed.section4_data_offset + parsed.section4_data_length];
    let values = decode_uncompressed_values(&parsed, &tables, section4).unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].descriptor, 4001);
    assert_eq!(values[0].raw_value, Some(2026.0));
}

fn test_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-data")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_eccodes_tables(table_dir: &PathBuf) {
    fs::create_dir_all(table_dir).unwrap();
    fs::write(
        table_dir.join("element.table"),
        "000001|tableA|table|TABLE A: ENTRY|CODE TABLE|0|0|6|NA|0|0\n\
         000002|tableB|long|EXAMPLE VALUE 2|Numeric|0|0|8|NA|0|0\n\
         000003|tableC|long|EXAMPLE VALUE 3|Numeric|0|0|8|NA|0|0\n\
         004001|year|long|YEAR|a|0|0|12|NA|0|0\n",
    )
    .unwrap();
    fs::write(
        table_dir.join("sequence.def"),
        "\"300002\" = [ 000002, 000003 ]\n\
         \"307080\" = [ 004001 ]\n",
    )
    .unwrap();
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

fn minimal_numeric_message() -> Vec<u8> {
    let mut message = minimal_message();
    message[6] = 49;
    message[37] = 0b0000_0100;
    message[38] = 0b0000_0001;
    message[41] = 6;
    message.insert(43, 0x7e);
    message.insert(44, 0xa0);
    message
}
