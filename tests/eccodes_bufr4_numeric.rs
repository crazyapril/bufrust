use bufrust::{decode_uncompressed_values, parse_message, split_messages, BufrError, TableSet};
use std::fs;
use std::path::{Path, PathBuf};

const MISSING: f64 = -1.0e100;

#[test]
fn bufr4_numeric_values_match_eccodes_refs() {
    let Some(root) = eccodes_root() else {
        eprintln!("BUFRUST_ECCODES_ROOT is not set; skipping ecCodes numeric reference test");
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
        if !name.ends_with(".bufr.num.ref") {
            continue;
        }

        let bufr_name = name.trim_end_matches(".num.ref");
        if should_skip_numeric_ref(bufr_name) {
            continue;
        }

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

        let tables = TableSet::from_eccodes_definitions(&definitions, &message).unwrap();
        let section4_end = message.section4_data_offset + message.section4_data_length;
        let section4 = &first_message[message.section4_data_offset..section4_end];
        let got = decode_uncompressed_values(&message, &tables, section4)
            .unwrap_or_else(|err| panic!("{bufr_name}: {err}"))
            .into_iter()
            .map(|value| value.raw_value.unwrap_or(MISSING))
            .collect::<Vec<_>>();
        let expected = read_ref(&path);

        assert!(
            same_values(&got, &expected),
            "{bufr_name}: got {} expected {}, first diff {:?}",
            got.len(),
            expected.len(),
            got.iter()
                .zip(expected.iter())
                .position(|(left, right)| !same_number(*left, *right))
        );
        checked += 1;
    }

    assert!(checked > 0, "no BUFR4 numeric refs were checked");
}

fn should_skip_numeric_ref(bufr_name: &str) -> bool {
    // ecCodes' bufrdc_ref.sh excludes this sample because its numeric reference is wrong.
    bufr_name == "uegabe.bufr"
}

fn read_ref(path: &Path) -> Vec<f64> {
    fs::read_to_string(path)
        .unwrap()
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn same_values(left: &[f64], right: &[f64]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(left, right)| same_number(*left, *right))
}

fn same_number(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-9 * right.abs().max(1.0)
}

fn eccodes_root() -> Option<PathBuf> {
    std::env::var_os("BUFRUST_ECCODES_ROOT").map(PathBuf::from)
}
