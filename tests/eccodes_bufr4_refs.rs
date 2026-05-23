use bufrust::{parse_message, split_messages, BufrError, TableSet};
use std::fs;
use std::path::PathBuf;

#[test]
fn bufr4_descriptors_match_eccodes_refs() {
    let Some(root) = eccodes_root() else {
        eprintln!("BUFRUST_ECCODES_ROOT is not set; skipping ecCodes descriptor reference test");
        return;
    };
    let data_dir = root.join("data").join("bufr");
    let definitions = root.join("definitions");

    let mut checked = 0usize;
    for entry in fs::read_dir(&data_dir).unwrap() {
        let path = entry.unwrap().path();
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if !name.ends_with(".bufr.desc.ref") {
            continue;
        }

        let bufr_name = name.trim_end_matches(".desc.ref");
        let bufr_path = data_dir.join(bufr_name);
        let bytes = fs::read(&bufr_path).unwrap();
        let chunks = split_messages(&bytes).unwrap();
        let Some(first_message) = chunks.first() else {
            continue;
        };
        let message = match parse_message(first_message) {
            Ok(message) => message,
            Err(BufrError::UnsupportedEdition(_)) => continue,
            Err(err) => panic!("{bufr_name}: {err}"),
        };

        let table_dir = definitions
            .join("bufr")
            .join("tables")
            .join(message.master_table_number.to_string())
            .join("wmo")
            .join(message.master_tables_version_number.to_string());
        let tables = TableSet::from_eccodes_dir(table_dir).unwrap();
        let got = tables.expand(&message.unexpanded_descriptors).unwrap();
        let expected = fs::read_to_string(&path)
            .unwrap()
            .split_whitespace()
            .map(|item| item.parse::<u32>().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(got, expected, "{bufr_name}");
        checked += 1;
    }

    assert!(checked > 0, "no BUFR4 descriptor refs were checked");
}

fn eccodes_root() -> Option<PathBuf> {
    std::env::var_os("BUFRUST_ECCODES_ROOT").map(PathBuf::from)
}
