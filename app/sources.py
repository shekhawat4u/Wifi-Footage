from __future__ import annotations

import asyncio
import math
import random
import time
from collections.abc import AsyncIterator

from .models import CSIFrame
from .parser import ParseError, parse_csi_line


class SimulationController:
    def __init__(self, config: dict, num_people: int = 2):
        self.config = config
        self.num_people = num_people
        self.start_time = time.monotonic()

    def get_people_positions(self, elapsed: float) -> list[dict]:
        if elapsed <= 5.0 or self.num_people <= 0:
            return []

        w = self.config["width_m"]
        h = self.config["height_m"]
        people = []

        # Person 1 (Smooth looping path)
        if self.num_people >= 1:
            p1_x = w * (0.5 + 0.35 * math.sin(elapsed * 0.45))
            p1_y = h * (0.5 + 0.32 * math.cos(elapsed * 0.32))
            people.append({
                "id": "person-1",
                "label": "Person 1",
                "color": "#69e7ff",
                "x": round(p1_x, 2),
                "y": round(p1_y, 2),
                "speed": round(0.6 + 0.2 * math.sin(elapsed * 0.5), 2),
            })

        # Person 2 (Diagonal & transverse crossing path)
        if self.num_people >= 2:
            p2_x = w * (0.5 + 0.38 * math.cos(elapsed * 0.38 + 1.2))
            p2_y = h * (0.5 + 0.35 * math.sin(elapsed * 0.55 + 0.8))
            people.append({
                "id": "person-2",
                "label": "Person 2",
                "color": "#ff7657",
                "x": round(p2_x, 2),
                "y": round(p2_y, 2),
                "speed": round(0.8 + 0.3 * math.cos(elapsed * 0.4), 2),
            })

        # Person 3 (Perimeter / corner inspection path)
        if self.num_people >= 3:
            p3_x = w * (0.5 + 0.40 * math.sin(elapsed * 0.28 + 2.5))
            p3_y = h * (0.5 + 0.40 * math.sin(elapsed * 0.56 + 1.5))
            people.append({
                "id": "person-3",
                "label": "Person 3",
                "color": "#ffcc66",
                "x": round(p3_x, 2),
                "y": round(p3_y, 2),
                "speed": round(0.5 + 0.2 * math.sin(elapsed * 0.3), 2),
            })

        return people


async def simulated_frames(config: dict, controller: SimulationController | None = None) -> AsyncIterator[tuple[CSIFrame, list[dict]]]:
    rng = random.Random(42)
    links = config["links"]
    sim = controller or SimulationController(config)
    start = time.monotonic()
    tick = 0

    while True:
        elapsed = time.monotonic() - start
        active_people = sim.get_people_positions(elapsed)

        for link in links:
            total_disturbance = 0.0
            min_dist = 999.0

            for p in active_people:
                dist = math.hypot(p["x"] - link["x"], p["y"] - link["y"])
                min_dist = min(min_dist, dist)
                if dist < 3.8:
                    total_disturbance += (3.8 - dist) * (4.2 + 2.0 * math.sin(elapsed * 2.2))

            if not active_people:
                min_dist = 4.0

            iq: list[int] = []
            for subcarrier in range(64):
                phase = subcarrier * 0.21 + tick * 0.025
                amp = 34 + min(total_disturbance, 18.0) * math.sin(subcarrier * 0.37 + elapsed)
                iq.extend([
                    round(amp * math.sin(phase) + rng.gauss(0, 0.8)),
                    round(amp * math.cos(phase) + rng.gauss(0, 0.8)),
                ])

            frame = CSIFrame(
                link["id"],
                int(time.time() * 1000),
                "02:00:00:00:00:01",
                round(-48 - min_dist + rng.gauss(0, 0.3)),
                -96,
                6,
                iq,
            )
            yield frame, active_people

        tick += 1
        await asyncio.sleep(0.04)


class UDPReceiverProtocol(asyncio.DatagramProtocol):
    def __init__(self, queue: asyncio.Queue[tuple[CSIFrame, list[dict]]]):
        self.queue = queue

    def datagram_received(self, data: bytes, addr: tuple[str, int]):
        text = data.decode("utf-8", "ignore")
        for line in text.strip().splitlines():
            try:
                frame = parse_csi_line(line)
                self.queue.put_nowait((frame, []))
            except (ParseError, asyncio.QueueFull):
                pass


async def udp_frames(host: str = "0.0.0.0", port: int = 5005) -> AsyncIterator[tuple[CSIFrame, list[dict]]]:
    queue: asyncio.Queue[tuple[CSIFrame, list[dict]]] = asyncio.Queue(maxsize=1000)
    loop = asyncio.get_running_loop()
    transport, _ = await loop.create_datagram_endpoint(
        lambda: UDPReceiverProtocol(queue),
        local_addr=(host, port)
    )
    try:
        while True:
            item = await queue.get()
            yield item
    finally:
        transport.close()


async def hybrid_frames(config: dict, sim_controller: SimulationController, specs: list[str], udp_port: int = 5005) -> AsyncIterator[tuple[CSIFrame | None, list[dict]]]:
    queue: asyncio.Queue[tuple[CSIFrame | None, list[dict]]] = asyncio.Queue(maxsize=1000)
    loop = asyncio.get_running_loop()

    # UDP datagram server for wireless ESP32s
    try:
        transport, _ = await loop.create_datagram_endpoint(
            lambda: UDPReceiverProtocol(queue),
            local_addr=("0.0.0.0", udp_port)
        )
    except Exception:
        transport = None

    # Resilient Serial reader tasks
    serial_tasks = []
    if specs:
        import serial_asyncio
        async def read_port(spec: str) -> None:
            port, _, receiver = spec.partition(":")
            receiver = receiver or "receiver-1"
            baud = 921600 if receiver == "receiver-1" else 115200
            while True:
                try:
                    reader, _ = await serial_asyncio.open_serial_connection(url=port, baudrate=baud)
                    while True:
                        raw = await reader.readline()
                        if not raw:
                            break
                        try:
                            frame = parse_csi_line(raw.decode("utf-8", "ignore"), receiver)
                            await queue.put((frame, []))
                        except (ParseError, asyncio.QueueFull):
                            continue
                except Exception:
                    await asyncio.sleep(2)
        serial_tasks = [asyncio.create_task(read_port(s)) for s in specs]

    try:
        while True:
            try:
                # Wait for real hardware frame
                item = await asyncio.wait_for(queue.get(), timeout=6.0)
                yield item
            except asyncio.TimeoutError:
                # No hardware frames arriving -> yield None to trigger OFFLINE status
                yield None, []
    finally:
        if transport:
            transport.close()
        for t in serial_tasks:
            t.cancel()


async def serial_frames(specs: list[str]) -> AsyncIterator[tuple[CSIFrame, list[dict]]]:
    try:
        import serial_asyncio
    except ImportError as exc:
        raise RuntimeError("Install pyserial-asyncio for asynchronous serial ingestion") from exc

    queue: asyncio.Queue[tuple[CSIFrame, list[dict]]] = asyncio.Queue(maxsize=500)

    async def read_port(spec: str) -> None:
        port, _, receiver = spec.partition(":")
        receiver = receiver or "receiver-1"
        reader, _ = await serial_asyncio.open_serial_connection(url=port, baudrate=921600)
        while True:
            raw = await reader.readline()
            try:
                frame = parse_csi_line(raw.decode("utf-8", "ignore"), receiver)
                await queue.put((frame, []))
            except (ParseError, asyncio.QueueFull):
                continue

    tasks = [asyncio.create_task(read_port(s)) for s in specs]
    try:
        while True:
            yield await queue.get()
    finally:
        for task in tasks:
            task.cancel()




