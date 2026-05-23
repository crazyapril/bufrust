use bufrust::{decode_uncompressed_values, parse_message, split_messages, BufrError, TableSet};
use std::fs;
use std::path::{Path, PathBuf};

const MISSING: f64 = -1.0e100;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("BUFRUST_ECCODES_ROOT").map(PathBuf::from))
        .ok_or("pass an ecCodes source root or set BUFRUST_ECCODES_ROOT")?;
    let data_dir = root.join("data/bufr");
    let definitions = root.join("definitions");

    let mut passed = 0usize;
    let mut failed = Vec::new();
    let mut unsupported = 0usize;

    for entry in fs::read_dir(&data_dir)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.ends_with(".bufr.num.ref") {
            continue;
        }

        let bufr_name = name.trim_end_matches(".num.ref");
        if should_skip_numeric_ref(bufr_name) {
            unsupported += 1;
            continue;
        }
        let bufr_path = data_dir.join(bufr_name);
        let raw = fs::read(&bufr_path)?;
        let chunks = split_messages(&raw)?;
        let Some(first) = chunks.first() else {
            unsupported += 1;
            continue;
        };

        let message = match parse_message(first) {
            Ok(message) => message,
            Err(BufrError::UnsupportedEdition(_)) => {
                unsupported += 1;
                continue;
            }
            Err(err) => {
                failed.push(format!("{bufr_name}: {err}"));
                continue;
            }
        };
        if should_debug(bufr_name) {
            println!(
                "debug {bufr_name}: edition={} subsets={} compressed={} sec4_bits={} descriptors={:?}",
                message.edition,
                message.number_of_subsets,
                message.compressed_data,
                message.section4_data_length * 8,
                message.unexpanded_descriptors
            );
        }

        let tables = TableSet::from_eccodes_definitions(&definitions, &message)?;
        let section4_end = message.section4_data_offset + message.section4_data_length;
        let section4 = &first[message.section4_data_offset..section4_end];
        let values = match decode_uncompressed_values(&message, &tables, section4) {
            Ok(values) => values,
            Err(err) => {
                failed.push(format!("{bufr_name}: {err}"));
                continue;
            }
        };
        let got = values
            .iter()
            .map(|value| value.value.unwrap_or(MISSING))
            .collect::<Vec<_>>();
        let expected = read_ref(&path)?;

        if same_values(&got, &expected) {
            passed += 1;
        } else {
            let first_diff = got
                .iter()
                .zip(expected.iter())
                .position(|(left, right)| !same_number(*left, *right));
            failed.push(format!(
                "{bufr_name}: got {} expected {}, first diff {:?}",
                got.len(),
                expected.len(),
                first_diff
            ));
            if should_debug(bufr_name) {
                print_debug(bufr_name, first_diff, &got, &expected, &values);
            }
        }
    }

    println!(
        "numeric refs: passed={passed} failed={} unsupported={unsupported}",
        failed.len()
    );
    for line in failed.iter().take(30) {
        println!("{line}");
    }
    if !failed.is_empty() {
        return Err(format!("{} numeric reference checks failed", failed.len()).into());
    }
    Ok(())
}

fn should_skip_numeric_ref(bufr_name: &str) -> bool {
    // ecCodes' own bufrdc_ref.sh excludes this sample because its numeric reference is wrong.
    bufr_name == "uegabe.bufr"
}

fn should_debug(bufr_name: &str) -> bool {
    std::env::var("BUFRUST_DEBUG_NUMERIC")
        .ok()
        .is_some_and(|names| names.split(',').any(|name| name.trim() == bufr_name))
}

fn print_debug(
    bufr_name: &str,
    first_diff: Option<usize>,
    got: &[f64],
    expected: &[f64],
    values: &[bufrust::DecodedValue],
) {
    let Some(index) = first_diff else {
        return;
    };
    let start = index.saturating_sub(8);
    let end = (index + 16)
        .min(got.len())
        .min(expected.len())
        .min(values.len());
    println!("debug {bufr_name} around index {index}");
    for offset in start..end {
        let value = &values[offset];
        println!(
            "{offset}: {:06} got={} expected={} text={:?} name={}",
            value.descriptor, got[offset], expected[offset], value.text, value.name
        );
    }
}

fn read_ref(path: &Path) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
    Ok(fs::read_to_string(path)?
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<Vec<_>, _>>()?)
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
