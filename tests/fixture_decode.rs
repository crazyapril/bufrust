use bufrust::{decode_values_with_builtin_tables, parse_messages, split_messages};

#[test]
fn decodes_ecmwf_cyclone_tracks_fixture() {
    let bytes = include_bytes!("fixtures/ecmwf_cyclone_tracks.bufr");
    let messages = parse_messages(bytes).unwrap();

    assert_eq!(messages.len(), 45);
    assert_eq!(messages[0].number_of_subsets, 51);
    assert!(messages[0].compressed_data);

    let mut total_values = 0usize;
    for chunk in split_messages(bytes).unwrap() {
        total_values += decode_values_with_builtin_tables(chunk).unwrap().len();
    }

    assert_eq!(total_values, 2_413_286);
}
