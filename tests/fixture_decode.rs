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

#[test]
fn decodes_rjtd_drifting_buoy_fixture() {
    let bytes = include_bytes!("fixtures/rjtd_drifting_buoy.bufr");
    let messages = parse_messages(bytes).unwrap();

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].originating_centre, 34);
    assert_eq!(messages[0].data_category, 1);
    assert_eq!(messages[0].international_data_subcategory, 25);
    assert_eq!(messages[0].unexpanded_descriptors, vec![315009]);

    let values = decode_values_with_builtin_tables(bytes).unwrap();
    assert_eq!(values.len(), 48);
    assert_eq!(values[0].descriptor, 001087);
    assert_eq!(values[0].value, Some(2_102_606.0));
}
