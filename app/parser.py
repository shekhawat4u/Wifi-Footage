from __future__ import annotations

import csv
import time

from .models import CSIFrame


class ParseError(ValueError):
    pass


def parse_csi_line(line: str, fallback_receiver: str = "receiver-1") -> CSIFrame:
    """Parse the project's compact CSV format.

    Preferred format:
    CSI_DATA,receiver,unix_ms,mac,rssi,noise,channel,len,i0,q0,...

    A fallback receiver is useful when one serial port represents one receiver.
    """
    try:
        row = next(csv.reader([line.strip()]))
    except (csv.Error, StopIteration) as exc:
        raise ParseError("invalid CSV") from exc
    if not row or row[0] != "CSI_DATA":
        raise ParseError("not a CSI_DATA record")

    # Compact format with an explicit receiver ID.
    if len(row) >= 9 and not _looks_numeric(row[1]):
        receiver, ts, mac = row[1], row[2], row[3]
        rssi, noise, channel, declared = row[4:8]
        raw_iq = row[8:]
    else:
        raise ParseError("unsupported CSI_DATA layout")

    try:
        values = [int(v.strip().strip("[]")) for v in raw_iq if v.strip().strip("[]")]
        expected = int(declared)
        if expected and len(values) < expected:
            raise ParseError(f"expected {expected} IQ bytes, got {len(values)}")
        return CSIFrame(
            receiver_id=receiver or fallback_receiver,
            timestamp_ms=int(ts) if ts else int(time.time() * 1000),
            source_mac=mac,
            rssi=int(rssi),
            noise_floor=int(noise),
            channel=int(channel),
            iq=values[:expected] if expected else values,
        )
    except (TypeError, ValueError) as exc:
        if isinstance(exc, ParseError):
            raise
        raise ParseError("invalid numeric field") from exc


def _looks_numeric(value: str) -> bool:
    try:
        float(value)
        return True
    except ValueError:
        return False

