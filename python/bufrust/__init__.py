"""Friendly Python interface for the Rust BUFR edition 4 decoder."""

from __future__ import annotations

from dataclasses import dataclass
from importlib.resources import files
from pathlib import Path
from typing import Any, Iterable, Iterator

from ._bufrust import (
    BufrMessage as RawMessage,
    DecodedValue,
    Descriptor,
    ElementDefinition,
    SequenceDefinition,
    TableSet,
    decode_values as _decode_values,
    decode_values_with_definitions as _decode_values_with_definitions,
    decode_values_with_tables as _decode_values_with_tables,
    expand_descriptors,
    parse_all_bytes as _parse_all_bytes,
    parse_all_file,
    parse_bytes as _parse_bytes,
    parse_file,
)

__version__ = "1.0.0"


class BufrustError(ValueError):
    """Raised by the high-level Python convenience API."""


@dataclass(frozen=True)
class Message:
    """A single BUFR message plus its original bytes."""

    raw: RawMessage
    data: bytes
    index: int = 0
    path: str | None = None
    definitions: str | None = None
    table_dir: str | None = None

    def decode(
        self,
        *,
        definitions: str | Path | None = None,
        table_dir: str | Path | None = None,
    ) -> list[DecodedValue]:
        """Decode Section 4 values for this message."""

        definitions = _coalesce_path(definitions, self.definitions)
        table_dir = _coalesce_path(table_dir, self.table_dir)
        if definitions is None and table_dir is None:
            definitions = builtin_definitions_path()
        if definitions is not None:
            return _decode_values_with_definitions(self.data, definitions)
        if table_dir is not None:
            return _decode_values(self.data, table_dir)
        raise BufrustError("decode() needs definitions=... or table_dir=...")

    def decode_with_tables(self, tables: TableSet) -> list[DecodedValue]:
        """Decode Section 4 values with an already loaded TableSet."""

        return _decode_values_with_tables(self.data, tables)

    @property
    def descriptors(self) -> list[int]:
        """Unexpanded Section 3 descriptor codes."""

        return list(self.raw.unexpanded_descriptors)

    @property
    def metadata(self) -> dict[str, Any]:
        """Message metadata as a plain dictionary."""

        fields = [
            "total_length",
            "edition",
            "section1_length",
            "master_table_number",
            "originating_centre",
            "originating_subcentre",
            "update_sequence_number",
            "local_section_present",
            "data_category",
            "international_data_subcategory",
            "local_data_subcategory",
            "master_tables_version_number",
            "local_tables_version_number",
            "typical_year",
            "typical_month",
            "typical_day",
            "typical_hour",
            "typical_minute",
            "typical_second",
            "section2_length",
            "section3_length",
            "number_of_subsets",
            "observed_data",
            "compressed_data",
            "section4_length",
            "section4_data_offset",
            "section4_data_length",
        ]
        out = {field: getattr(self.raw, field) for field in fields}
        out["message_index"] = self.index
        if self.path is not None:
            out["path"] = self.path
        return out

    def to_dict(self, *, decode: bool = False, **decode_options: Any) -> dict[str, Any]:
        """Convert metadata, descriptors, and optionally decoded values to dictionaries."""

        out = {
            "metadata": self.metadata,
            "descriptors": self.descriptors,
        }
        if decode:
            out["values"] = values_to_dicts(self.decode(**decode_options))
        return out

    def to_dataframe(self, **decode_options: Any):
        """Decode values into a pandas DataFrame if pandas is installed."""

        return values_to_dataframe(
            self.decode(**decode_options),
            number_of_subsets=self.raw.number_of_subsets,
            message_index=self.index,
        )

    def __repr__(self) -> str:
        return (
            "Message("
            f"index={self.index}, edition={self.raw.edition}, "
            f"subsets={self.raw.number_of_subsets}, "
            f"compressed={self.raw.compressed_data})"
        )


class Dataset:
    """A BUFR file or byte buffer containing one or more messages."""

    def __init__(
        self,
        data: bytes,
        *,
        path: str | None = None,
        definitions: str | Path | None = None,
        table_dir: str | Path | None = None,
    ) -> None:
        self.data = bytes(data)
        self.path = path
        self.definitions = _path_str(definitions)
        self.table_dir = _path_str(table_dir)
        raw_messages = _parse_all_bytes(self.data)
        chunks = _split_message_bytes(self.data)
        if len(raw_messages) != len(chunks):
            raise BufrustError("internal split mismatch while reading BUFR messages")
        self.messages = [
            Message(
                raw=raw,
                data=chunk,
                index=index,
                path=path,
                definitions=self.definitions,
                table_dir=self.table_dir,
            )
            for index, (raw, chunk) in enumerate(zip(raw_messages, chunks))
        ]

    @classmethod
    def from_file(
        cls,
        path: str | Path,
        *,
        definitions: str | Path | None = None,
        table_dir: str | Path | None = None,
    ) -> "Dataset":
        """Read BUFR messages from a file path."""

        path = Path(path)
        return cls(
            path.read_bytes(),
            path=str(path),
            definitions=definitions,
            table_dir=table_dir,
        )

    def decode(
        self,
        message: int = 0,
        *,
        definitions: str | Path | None = None,
        table_dir: str | Path | None = None,
    ) -> list[DecodedValue]:
        """Decode one message by index."""

        return self.messages[message].decode(definitions=definitions, table_dir=table_dir)

    def decode_all(
        self,
        *,
        definitions: str | Path | None = None,
        table_dir: str | Path | None = None,
    ) -> list[list[DecodedValue]]:
        """Decode every message in the dataset."""

        definitions = _coalesce_path(definitions, self.definitions)
        table_dir = _coalesce_path(table_dir, self.table_dir)
        if table_dir is not None:
            tables = TableSet.from_eccodes(table_dir)
            return [message.decode_with_tables(tables) for message in self.messages]

        definitions = definitions or builtin_definitions_path()
        cache: dict[tuple[int, int, int, int, int], TableSet] = {}
        decoded = []
        for message in self.messages:
            raw = message.raw
            key = (
                raw.master_table_number,
                raw.master_tables_version_number,
                raw.local_tables_version_number,
                raw.originating_centre,
                raw.originating_subcentre,
            )
            tables = cache.get(key)
            if tables is None:
                tables = TableSet.from_definitions(definitions, raw)
                cache[key] = tables
            decoded.append(message.decode_with_tables(tables))
        return decoded

    @property
    def metadata(self) -> list[dict[str, Any]]:
        """Metadata for all messages."""

        return [message.metadata for message in self.messages]

    @property
    def descriptors(self) -> list[list[int]]:
        """Unexpanded descriptors for all messages."""

        return [message.descriptors for message in self.messages]

    def to_dict(self, *, decode: bool = False, **decode_options: Any) -> dict[str, Any]:
        """Convert the dataset to dictionaries."""

        return {
            "path": self.path,
            "message_count": len(self.messages),
            "messages": [
                message.to_dict(decode=decode, **decode_options)
                for message in self.messages
            ],
        }

    def to_dataframe(self, message: int | None = None, **decode_options: Any):
        """Decode one or all messages into a pandas DataFrame if pandas is installed."""

        if message is not None:
            return self.messages[message].to_dataframe(**decode_options)
        decoded = self.decode_all(**decode_options)
        try:
            import pandas as pd
        except ImportError as exc:
            raise BufrustError("to_dataframe() requires pandas") from exc
        frames = [
            values_to_dataframe(
                values,
                number_of_subsets=message.raw.number_of_subsets,
                message_index=message.index,
            )
            for message, values in zip(self.messages, decoded)
        ]
        return pd.concat(frames, ignore_index=True) if frames else pd.DataFrame()

    def __iter__(self) -> Iterator[Message]:
        return iter(self.messages)

    def __getitem__(self, index: int) -> Message:
        return self.messages[index]

    def __len__(self) -> int:
        return len(self.messages)

    def __repr__(self) -> str:
        source = f", path={self.path!r}" if self.path else ""
        return f"Dataset(messages={len(self.messages)}{source})"


def open(
    path: str | Path,
    *,
    definitions: str | Path | None = None,
    table_dir: str | Path | None = None,
) -> Dataset:
    """Open a BUFR file and return a Dataset."""

    return Dataset.from_file(path, definitions=definitions, table_dir=table_dir)


def loads(
    data: bytes | bytearray | memoryview,
    *,
    definitions: str | Path | None = None,
    table_dir: str | Path | None = None,
) -> Dataset:
    """Load BUFR messages from bytes and return a Dataset."""

    return Dataset(bytes(data), definitions=definitions, table_dir=table_dir)


def load(
    source: str | Path | bytes | bytearray | memoryview,
    *,
    definitions: str | Path | None = None,
    table_dir: str | Path | None = None,
) -> Dataset:
    """Open a path or bytes-like object."""

    if isinstance(source, (bytes, bytearray, memoryview)):
        return loads(source, definitions=definitions, table_dir=table_dir)
    return open(source, definitions=definitions, table_dir=table_dir)


def values_to_dicts(values: Iterable[DecodedValue]) -> list[dict[str, Any]]:
    """Convert decoded values to plain dictionaries."""

    return [
        {
            "descriptor": value.descriptor,
            "name": value.name,
            "value": value.value,
            "raw": value.raw,
            "text": value.text,
        }
        for value in values
    ]


def values_to_dataframe(
    values: Iterable[DecodedValue],
    *,
    number_of_subsets: int | None = None,
    message_index: int | None = None,
):
    """Convert decoded values to a long-form pandas DataFrame."""

    try:
        import pandas as pd
    except ImportError as exc:
        raise BufrustError("values_to_dataframe() requires pandas") from exc

    rows = values_to_dicts(values)
    if number_of_subsets and rows and len(rows) % number_of_subsets == 0:
        values_per_subset = len(rows) // number_of_subsets
        for offset, row in enumerate(rows):
            row["subset"] = offset // values_per_subset
            row["position"] = offset % values_per_subset
    else:
        for offset, row in enumerate(rows):
            row["subset"] = None
            row["position"] = offset
    if message_index is not None:
        for row in rows:
            row["message"] = message_index
    return pd.DataFrame(rows)


def builtin_definitions_path() -> str:
    """Return the bundled ecCodes-style BUFR definitions directory."""

    return str(files(__package__).joinpath("definitions"))


def tables_for_message(
    message: Message | RawMessage,
    *,
    definitions: str | Path | None = None,
) -> TableSet:
    """Load the table set selected by a message header.

    Uses the bundled WMO definitions by default. Pass ``definitions=`` to use
    an external ecCodes definitions root instead.
    """

    raw = message.raw if isinstance(message, Message) else message
    return TableSet.from_definitions(
        _path_str(definitions) or builtin_definitions_path(),
        raw,
    )


def _split_message_bytes(data: bytes) -> list[bytes]:
    chunks = []
    offset = 0
    while offset < len(data):
        start = data.find(b"BUFR", offset)
        if start < 0:
            break
        if start + 8 > len(data):
            raise BufrustError("truncated BUFR header")
        total_length = int.from_bytes(data[start + 4 : start + 7], "big")
        end = start + total_length
        if end > len(data):
            raise BufrustError("truncated BUFR message")
        chunks.append(data[start:end])
        offset = end
    return chunks


def _path_str(path: str | Path | None) -> str | None:
    return None if path is None else str(path)


def _coalesce_path(value: str | Path | None, default: str | None) -> str | None:
    return _path_str(value) if value is not None else default


read = open
get = open
parse_bytes = _parse_bytes
parse_all_bytes = _parse_all_bytes
decode_values = _decode_values
decode_values_with_definitions = _decode_values_with_definitions
decode_values_with_tables = _decode_values_with_tables
BufrMessage = RawMessage

__all__ = [
    "BufrustError",
    "Dataset",
    "Message",
    "RawMessage",
    "BufrMessage",
    "DecodedValue",
    "Descriptor",
    "ElementDefinition",
    "SequenceDefinition",
    "TableSet",
    "builtin_definitions_path",
    "decode_values",
    "decode_values_with_definitions",
    "decode_values_with_tables",
    "expand_descriptors",
    "get",
    "load",
    "loads",
    "open",
    "parse_all_bytes",
    "parse_all_file",
    "parse_bytes",
    "parse_file",
    "read",
    "tables_for_message",
    "values_to_dataframe",
    "values_to_dicts",
]
