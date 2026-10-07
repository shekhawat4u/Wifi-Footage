from __future__ import annotations

from dataclasses import dataclass, field
from typing import Sequence


@dataclass(slots=True)
class CSIFrame:
    receiver_id: str
    timestamp_ms: int
    source_mac: str
    rssi: int
    noise_floor: int
    channel: int
    iq: Sequence[int]


@dataclass(slots=True)
class LinkState:
    id: str
    score: float = 0.0
    rssi: float = -100.0
    samples: int = 0
    calibrated: bool = False
    history: list[float] = field(default_factory=list)

