# bufrust

`bufrust` is a BUFR edition 4 parser and data decoder written in Rust, with a
Python API designed to feel natural in data workflows.

It can:

- parse one or more BUFR edition 4 messages from files or bytes
- expose Section 0/1/2/3/4 metadata
- read unexpanded descriptors from Section 3
- use bundled WMO/ecCodes-style `element.table` and `sequence.def` definitions
- load external ecCodes definitions or local/custom tables when needed
- load BUFR4-45 CSV-style Table B and Table D files
- expand Table D sequences and replication descriptors
- decode Section 4 values for uncompressed and compressed BUFR4 messages
- provide Python helpers such as `bufrust.open(...)`, `Dataset`, `Message`,
  `to_dict()`, and optional `to_dataframe()`

The Rust core is intentionally small and explicit. The Python layer adds a
friendlier interface for interactive use and notebooks.

## Status

`bufrust` 1.0.0 is intended as a usable BUFR4 decoding library for Python and
Rust. The decoder is covered by the ecCodes BUFR4 numeric and descriptor
reference data used during development, plus a real ECMWF cyclone tracks BUFR
fixture included in this repository.

`bufrust` bundles the WMO BUFR Table B/Table D files needed for normal
descriptor expansion and value decoding. You only need to pass table paths when
you want to override the bundled tables with local centre tables, a different
ecCodes release, or BUFR4-45 CSV files.

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

Rust:

```toml
[dependencies]
bufrust = "1"
```

## Quick Start In Python

Parse metadata only:

```python
import bufrust

ds = bufrust.open("sample.bufr")

print(ds)
print(ds.metadata[0])
print(ds.descriptors[0])
```

Decode values using the bundled WMO tables:

```python
import bufrust

ds = bufrust.open("sample.bufr")
values = ds.decode()

for value in values[:5]:
    print(value.descriptor, value.name, value.value, value.text)
```

Work with multiple BUFR messages:

```python
ds = bufrust.open("multi-message.bufr")

for message in ds:
    print(message.index, message.raw.number_of_subsets, message.descriptors)
    decoded = message.decode()
    print(len(decoded))
```

Load from bytes:

```python
payload = Path("sample.bufr").read_bytes()
ds = bufrust.loads(payload)
```

Convert to dictionaries:

```python
record = ds[0].to_dict(decode=True)
print(record["metadata"])
print(record["values"][0])
```

## DataFrame Workflow

Install the optional pandas extra:

```bash
pip install "bufrust[dataframe]"
```

`to_dataframe()` is the most convenient way to inspect and filter decoded BUFR
data. It returns a long-form table where each decoded BUFR value is one row.

The repository includes a real ECMWF cyclone tracks BUFR4 file for examples and
regression tests:

```text
tests/fixtures/ecmwf_cyclone_tracks.bufr
```

Decode it directly into pandas:

```python
import bufrust

ds = bufrust.open("tests/fixtures/ecmwf_cyclone_tracks.bufr")
frame = ds.to_dataframe()

print(frame.head())
print(frame.shape)
```

`to_dataframe()` returns a long-form table with columns such as `descriptor`,
`name`, `value`, `raw`, `text`, `subset`, `position`, and `message`.

For `ecmwf_cyclone_tracks.bufr`, this produces 45 BUFR messages and more than
2.4 million decoded rows:

```text
(2413286, 8)
```

Typical analysis patterns:

```python
# All latitude rows.
latitudes = frame[frame["name"].str.contains("LATITUDE", case=False, na=False)]

# Values from one BUFR message and subset.
track0 = frame[(frame["message"] == 0) & (frame["subset"] == 0)]

# Keep only numeric values.
numeric = frame[frame["value"].notna()]
```

For large files, decode one message at a time:

```python
ds = bufrust.open("tests/fixtures/ecmwf_cyclone_tracks.bufr")
first = ds.to_dataframe(message=0)
```

If pandas is not installed, use dictionaries:

```python
record = ds[0].to_dict(decode=True)
print(record["values"][0])
```

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
`element.table` and `sequence.def`.

Important objects:

- `Dataset`: a file or byte buffer containing one or more messages
- `Message`: one parsed BUFR message and its original bytes
- `DecodedValue`: one decoded value with `descriptor`, `name`, `value`, `raw`,
  and `text`
- `TableSet`: loaded BUFR Table B/Table D definitions
- `Descriptor`: an F/X/Y descriptor helper

Low-level functions are also available:

```python
msg = bufrust.parse_file("sample.bufr")
messages = bufrust.parse_all_bytes(payload)

values = bufrust.open("sample.bufr").decode()
```

## Table Definitions

### Bundled WMO tables

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

The bundled files are the ecCodes-style WMO BUFR Table B/Table D files
(`element.table` and `sequence.def`) copied from ecCodes 2.47.0. The full
ecCodes project is licensed under Apache-2.0; its license and notice are
included under `python/bufrust/definitions/ECCODES_LICENSE` and
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
```

### BUFR4-45 CSV tables

If you have BUFR4-45 CSV files, load the directory containing files named like
`BUFRCREX_TableB_en_*.csv` and `BUFR_TableD_en_*.csv`:

```python
tables = bufrust.TableSet.from_bufr4_45("external/BUFR4-45")
print(tables.get_element(42001))
```

## Rust API

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
python scripts/bump-version.py v1.0.1
git add Cargo.toml Cargo.lock pyproject.toml python/bufrust/__init__.py
git commit -m "Release v1.0.1"
git tag v1.0.1
git push origin main v1.0.1
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

## Notes And Limitations

- BUFR editions other than edition 4 are rejected.
- WMO Table B/Table D definitions are bundled. Local centre tables and custom
  table versions may still need an explicit `definitions=` or `table_dir=`.
- `to_dataframe()` is optional and imports pandas only when called.
- The current decoded value model is long-form. Higher-level xarray-style
  dimensions and coordinates can be built on top of this API as the decoder
  matures.

## License

Apache-2.0.

The bundled ecCodes-derived BUFR table files are also Apache-2.0 and retain the
ecCodes notice files in `python/bufrust/definitions`.
