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

#[test]
fn decodes_rjtd_iucc10_fixture() {
    let bytes = include_bytes!("fixtures/rjtd_iucc10.bufr");
    let messages = parse_messages(bytes).unwrap();

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].originating_centre, 34);
    assert_eq!(messages[0].data_category, 12);
    assert_eq!(messages[0].master_tables_version_number, 12);
    assert_eq!(messages[0].typical_year, 2025);
    assert_eq!(messages[0].typical_month, 8);
    assert_eq!(messages[0].typical_day, 22);

    let values = decode_values_with_builtin_tables(bytes).unwrap();
    assert_eq!(values.len(), 32);
    assert_eq!(values[7].descriptor, 001007);
    assert_eq!(values[7].value, Some(174.0));
    assert_eq!(values[13].descriptor, 008005);
    assert_eq!(values[13].meaning.as_deref(), Some("STORM CENTRE"));
    assert_eq!(values[10].descriptor, 001027);
    assert_eq!(values[10].text.as_deref(), Some("nameless"));
    assert_eq!(values[14].descriptor, 005002);
    assert_eq!(values[14].value, Some(17.34));
    assert_eq!(values[15].descriptor, 006002);
    assert_eq!(values[15].value, Some(117.8));
}
