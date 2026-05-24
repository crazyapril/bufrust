# bufrust

`bufrust` is a BUFR edition 4 parser and data decoder written in Rust, with a
Python API designed to feel natural in data workflows.

It can:

- parse one or more BUFR edition 4 messages from files or bytes
- expose Section 0/1/2/3/4 metadata
- read unexpanded descriptors from Section 3
- use bundled ecCodes BUFR definitions, including WMO/local tables and
  code/flag tables
- load external ecCodes definitions or local/custom tables when needed
- load BUFR4-45 CSV-style Table B and Table D files
- expand Table D sequences and replication descriptors
- decode Section 4 values for uncompressed and compressed BUFR4 messages
- provide Python helpers such as `bufrust.open(...)`, `Dataset`, `Message`,
  `to_dict()`, and pandas-backed `to_dataframe()`

The Rust core is intentionally small and explicit. The Python layer adds a
friendlier interface for interactive use and notebooks.

## Status

`bufrust` is intended as a usable BUFR4 decoding library for Python and Rust.
The decoder is covered by the ecCodes BUFR4 numeric and descriptor reference
data used during development, plus real ECMWF and JMA/RJTD BUFR fixtures
included in this repository.

`bufrust` bundles the ecCodes BUFR definitions needed for descriptor expansion,
value decoding, and code/flag-table meanings. You only need to pass table paths
when you want to override the bundled tables with a different ecCodes release or
BUFR4-45 CSV files.

## Installation

Python, from a wheel:

```bash
pip install bufrust
```

Python, from a local checkout:

```bash
pip install maturin
maturin develop
```

## Quick Start In Python

Decode a BUFR file into plain Python dictionaries:

```python
import bufrust

ds = bufrust.open("sample.bufr")
record = ds[0].to_dict()

print(record["metadata"])
print(record["descriptors"])
print(record["values"][0])      # descriptor, name, data
```

`to_dict()` decodes values by default using the bundled ecCodes BUFR
definitions. Use `decode=False` when you only want metadata and descriptors:

```python
metadata_only = ds[0].to_dict(decode=False)
debug = ds[0].to_dict(raw=True)  # also include raw/raw_text/raw_meaning/raw_value
```

For multiple-message files, iterate over the dataset:

```python
ds = bufrust.open("multi-message.bufr")

for message in ds:
    print(message.index, message.raw.number_of_subsets, message.descriptors)
    decoded = message.decode()
    print(len(decoded))
```

Load from bytes:

```python
from pathlib import Path

payload = Path("sample.bufr").read_bytes()
ds = bufrust.loads(payload)
```

For pandas workflows, call `to_dataframe()`:

```python
ds = bufrust.open("tests/fixtures/ecmwf_cyclone_tracks.bufr")
frame = ds.to_dataframe()

print(frame[["descriptor", "name", "text", "value"]].head())
print(frame.shape)  # (2413286, 7) for the bundled ECMWF cyclone fixture
```

`to_dataframe()` returns one row per decoded BUFR value. Text-like data goes to
`text` (`raw_text` or `raw_meaning`), while numeric data goes to `value`; pass
`raw=True` to include the underlying raw fields.

```python
latitudes = frame[frame["name"].str.contains("LATITUDE", case=False, na=False)]
track0 = frame[(frame["message"] == 0) & (frame["subset"] == 0)]
first = ds.to_dataframe(message=0)
```

## Benchmark

The following benchmark uses
`tests/fixtures/ecmwf_cyclone_tracks.bufr`, a 914 KB BUFR4 file containing 45
messages and 2,413,286 decoded values. The `bufrust` runs include code/flag
table meaning lookup.

Indicative median times on a local Windows workstation:

| Library / operation | Median time | Output checked |
| --- | ---: | --- |
| `bufrust.open(path).decode_all()` | 0.757 s | 45 messages, 2,413,286 values, 149,430 meanings |
| ecCodes Python `unpack + numericValues/stringValues` | 0.772 s | 45 messages, 2,413,286 numeric values, 90 strings |
| pybufrkit `Decoder.process(...)` | 3.743 s | 45 messages, 2,413,286 values |
| `bufrust.open(path).to_dataframe()` | 3.111 s | DataFrame shape `(2413286, 7)` |

The ecCodes and pybufrkit rows use their normal low-level Python decode paths
and do not construct a pandas DataFrame or per-value text/meaning columns. The
ecCodes row calls `unpack` and reads `numericValues`/`stringValues`; it is
included as a strong reference point for decode throughput. The
`bufrust.decode_all()` and `bufrust.to_dataframe()` rows include code/flag table
meaning lookup, and the `to_dataframe()` row also includes pandas allocation for
the long-form table. By default that table keeps display text in `text` and
numeric data in `value`; use `raw=True` to expose `raw_text`, `raw_meaning`,
`raw_value`, and `raw`.

## Python API

The high-level API mirrors common Python data libraries:

```python
ds = bufrust.open(path, definitions=None, table_dir=None)
ds = bufrust.load(path_or_bytes, definitions=None, table_dir=None)
ds = bufrust.loads(bytes_data, definitions=None, table_dir=None)
```

Use `definitions=` when you have an ecCodes definitions root containing
`bufr/tables/...` and want to override the bundled tables. `bufrust` chooses
the WMO and local table directories from the message header.

Use `table_dir=` when you already know the exact table directory containing
`element.table`, `sequence.def`, and optional `codetables/`.

Decoded values expose a display-oriented `data` property. It is chosen in this
order: `raw_text`, then `raw_meaning`, then `raw_value`.

```python
record = ds[0].to_dict()                   # descriptor, name, data
metadata = ds[0].to_dict(decode=False)     # skip value decoding
debug = ds[0].to_dict(raw=True)
df = ds.to_dataframe()                     # text and value columns
debug_df = ds.to_dataframe(raw=True)
```

Important objects:

- `Dataset`: a file or byte buffer containing one or more messages
- `Message`: one parsed BUFR message and its original bytes
- `DecodedValue`: one decoded value with `descriptor`, `name`, `data`,
  `raw_text`, `raw_meaning`, `raw_value`, and `raw`
- `TableSet`: loaded BUFR Table B/Table D definitions plus code/flag tables
- `Descriptor`: an F/X/Y descriptor helper

Low-level functions are also available:

```python
msg = bufrust.parse_file("sample.bufr")
messages = bufrust.parse_all_bytes(payload)

values = bufrust.open("sample.bufr").decode()
```

## Table Definitions

### Bundled ecCodes BUFR definitions

For ordinary use, no table path is required:

```python
ds = bufrust.open("sample.bufr")
values = ds.decode()
```

The bundled definitions are available for inspection:

```python
print(bufrust.builtin_definitions_path())
tables = bufrust.tables_for_message(ds[0])
```

The bundled files are copied from ecCodes 2.47.0 and include BUFR templates,
WMO/local Table B and Table D files, version aliases, and `codetables` used for
code/flag-table meanings. The full ecCodes project is licensed under
Apache-2.0; its license and notice are included under
`python/bufrust/definitions/ECCODES_LICENSE` and
`python/bufrust/definitions/ECCODES_NOTICE`.

### External ecCodes-style tables

Load one concrete table directory:

```python
tables = bufrust.TableSet.from_eccodes(
    "external/eccodes/definitions/bufr/tables/0/wmo/42"
)
print(tables.expand([307080]))
```

Decode using an external ecCodes definitions root:

```python
ds = bufrust.open("sample.bufr", definitions="external/eccodes/definitions")
values = ds.decode()
```

The definitions root should look like:

```text
definitions/
  bufr/
    tables/
      0/
        wmo/
          42/
            element.table
            sequence.def
            codetables/
```

### BUFR4-45 CSV tables

If you have BUFR4-45 CSV files, load the directory containing files named like
`BUFRCREX_TableB_en_*.csv` and `BUFR_TableD_en_*.csv`:

```python
tables = bufrust.TableSet.from_bufr4_45("external/BUFR4-45")
print(tables.get_element(42001))
```

## Rust API

(Not yet published on crates.io)

```rust
use bufrust::{decode_values_with_builtin_tables, parse_message, TableSet};

fn main() -> bufrust::Result<()> {
    let bytes = std::fs::read("sample.bufr")?;
    let message = parse_message(&bytes)?;

    println!("edition {}", message.edition);
    println!("descriptors {:?}", message.unexpanded_descriptors);

    let values = decode_values_with_builtin_tables(&bytes)?;

    println!("decoded {} values", values.len());
    Ok(())
}
```

Descriptor expansion:

```rust
use bufrust::TableSet;

let message = bufrust::parse_message(&std::fs::read("sample.bufr")?)?;
let tables = TableSet::from_builtin_definitions(&message)?;
let expanded = tables.expand(&[307080])?;
```

## Development

Run the self-contained test suite:

```bash
cargo test
```

Build the Python wheel:

```bash
maturin build --release
```

Install the local Python package for development:

```bash
maturin develop
python -c "import bufrust; print(bufrust.__version__)"
```

Update versions before tagging a release:

```bash
python scripts/bump-version.py vX.Y.Z
git add Cargo.toml Cargo.lock pyproject.toml python/bufrust/__init__.py
git commit -m "Release vX.Y.Z"
git tag vX.Y.Z
git push origin main vX.Y.Z
```

## Optional ecCodes Reference Tests

The default tests do not require files outside this repository.

For deeper compatibility testing, place an ecCodes source tree or extracted
definitions/test-data tree inside this repository, then set `BUFRUST_ECCODES_ROOT`:

```powershell
$env:BUFRUST_ECCODES_ROOT = "external\eccodes-2.47.0"
cargo test
cargo run --bin check_eccodes_numeric -- external\eccodes-2.47.0
```

The helper script downloads the BUFR test payloads referenced by ecCodes into
`external/eccodes-2.47.0/data/bufr`:

```powershell
.\scripts\download-eccodes-bufr-tests.ps1
```

During development the BUFR4 numeric checker reports:

```text
numeric refs: passed=23 failed=0 unsupported=109
```

`unsupported` includes non-BUFR4 fixtures and `uegabe.bufr`, which ecCodes'
own reference script excludes because its numeric reference is incorrect.

## License

Apache-2.0.

The bundled ecCodes-derived BUFR table files are also Apache-2.0 and retain the
ecCodes notice files in `python/bufrust/definitions`.
