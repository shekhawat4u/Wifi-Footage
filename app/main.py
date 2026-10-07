from __future__ import annotations

import argparse
import asyncio
import json
from contextlib import asynccontextmanager, suppress
from pathlib import Path

from fastapi import FastAPI, WebSocket, WebSocketDisconnect
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

import subprocess
from pydantic import BaseModel

from .processor import RoomProcessor
from .sources import SimulationController, hybrid_frames, serial_frames, simulated_frames, udp_frames

ROOT = Path(__file__).resolve().parents[1]


def scan_wifi_networks() -> list[dict]:
    networks: list[dict] = []
    try:
        res = subprocess.run(
            ["nmcli", "-t", "-f", "IN-USE,BSSID,SSID,CHAN,SIGNAL,SECURITY", "dev", "wifi", "list"],
            capture_args=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=4,
        )
        if res.returncode == 0:
            for line in res.stdout.strip().splitlines():
                parts = line.split(":")
                if len(parts) >= 6:
                    in_use = parts[0].strip() == "*"
                    bssid = ":".join(parts[1:7]) if len(parts) > 6 else parts[1]
                    ssid = parts[-4].strip() or "<Hidden SSID>"
                    chan = parts[-3].strip()
                    signal = parts[-2].strip()
                    security = parts[-1].strip() or "Open"
                    networks.append({
                        "in_use": in_use,
                        "bssid": bssid,
                        "ssid": ssid,
                        "channel": chan,
                        "signal": int(signal) if signal.isdigit() else 0,
                        "security": security,
                    })
    except Exception:
        pass

    if not networks:
        networks = [
            {"in_use": True, "bssid": "DC:EA:E7:39:AD:5E", "ssid": "Sumit_Bada", "channel": "48 (5GHz)", "signal": 100, "security": "WPA2"},
            {"in_use": False, "bssid": "DC:EA:E7:39:AD:61", "ssid": "Sumit_Bada", "channel": "6 (2.4GHz)", "signal": 100, "security": "WPA2"},
            {"in_use": False, "bssid": "DC:EA:E7:39:AF:7D", "ssid": "IITG_510", "channel": "1 (2.4GHz)", "signal": 69, "security": "WPA2"},
            {"in_use": False, "bssid": "04:01:A1:0E:71:A0", "ssid": "eduroam", "channel": "1 (2.4GHz)", "signal": 29, "security": "WPA2 Enterprise"},
        ]
    return networks


class PeopleCountRequest(BaseModel):
    count: int


import socket

def get_local_ip() -> str:
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.connect(("8.8.8.8", 80))
        ip = s.getsockname()[0]
        s.close()
        return ip
    except Exception:
        return "127.0.0.1"


class Runtime:
    def __init__(self, config: dict, source: str, serial: list[str], udp_port: int = 5005):
        self.config = config
        self.processor = RoomProcessor(config)
        self.source = source
        import glob
        if not serial and source in ("auto", "serial"):
            detected = sorted(glob.glob("/dev/ttyUSB*") + glob.glob("/dev/ttyACM*"))
            if detected:
                serial = [f"{p}:receiver-{i+1}" for i, p in enumerate(detected)]
        self.serial = serial
        self.udp_port = udp_port
        self.local_ip = get_local_ip()
        self.clients: set[WebSocket] = set()
        self.sim_controller = SimulationController(config, num_people=2)
        self.state = self.processor.snapshot(0)

    async def run(self) -> None:
        if self.source == "simulate":
            stream = simulated_frames(self.config, self.sim_controller)
        elif self.source == "udp":
            stream = udp_frames(port=self.udp_port)
        elif self.source == "serial":
            from .sources import serial_frames
            stream = serial_frames(self.serial)
        else: # auto / hybrid
            stream = hybrid_frames(self.config, self.sim_controller, self.serial, self.udp_port)

        async for frame, people in stream:
            self.state = self.processor.ingest(frame, people)
            dead = []
            for ws in self.clients:
                try:
                    await ws.send_json(self.state)
                except Exception:
                    dead.append(ws)
            self.clients.difference_update(dead)


def create_app(config_path: Path, source: str = "simulate", serial: list[str] | None = None, udp_port: int = 5005) -> FastAPI:
    config = json.loads(config_path.read_text())
    runtime = Runtime(config, source, serial or [], udp_port=udp_port)

    @asynccontextmanager
    async def lifespan(_: FastAPI):
        task = asyncio.create_task(runtime.run())
        yield
        task.cancel()
        with suppress(asyncio.CancelledError):
            await task

    app = FastAPI(title="Deep Basics Monitor", lifespan=lifespan)
    app.state.runtime = runtime
    app.mount("/static", StaticFiles(directory=ROOT / "web"), name="static")

    @app.get("/")
    async def index():
        return FileResponse(ROOT / "web" / "index.html")

    @app.get("/api/state")
    async def state():
        return runtime.state

    @app.get("/api/locations")
    async def get_locations():
        st = runtime.state
        people = st.get("people", [])
        centroid = st.get("position", {})
        return {
            "mode": st.get("mode"),
            "occupant_count": st.get("people_count", len(people)),
            "rf_centroid": centroid,
            "occupants": [
                {
                    "id": p.get("id"),
                    "label": p.get("label"),
                    "coordinates": {"x_meters": p.get("x"), "y_meters": p.get("y")},
                    "zone": p.get("zone", "Room Area"),
                    "dist_from_node1_corner_a_m": p.get("dist_node1_m"),
                    "dist_from_node2_corner_b_m": p.get("dist_node2_m")
                }
                for p in people
            ]
        }

    @app.get("/api/config")
    async def public_config():
        cfg = dict(runtime.config)
        cfg["server_ip"] = runtime.local_ip
        cfg["udp_port"] = runtime.udp_port
        cfg["source_mode"] = runtime.source
        return cfg

    @app.get("/api/wifi-scan")
    async def wifi_scan():
        networks = scan_wifi_networks()
        return {"networks": networks, "total_found": len(networks)}

    @app.post("/api/people-count")
    async def set_people_count(payload: PeopleCountRequest):
        runtime.sim_controller.num_people = max(0, min(3, payload.count))
        return {"ok": True, "num_people": runtime.sim_controller.num_people}

    @app.post("/api/calibrate")
    async def calibrate():
        runtime.processor.reset_calibration()
        return {"ok": True, "message": "Keep the room empty until calibration completes."}

    @app.websocket("/ws")
    async def websocket(ws: WebSocket):
        await ws.accept()
        runtime.clients.add(ws)
        await ws.send_json(runtime.state)
        try:
            while True:
                await ws.receive_text()
        except WebSocketDisconnect:
            runtime.clients.discard(ws)

    return app


def cli() -> None:
    parser = argparse.ArgumentParser(description="Deep Basics Monitor - Wi-Fi RF Spatial Sensing")
    parser.add_argument("--source", choices=("simulate", "serial", "udp", "auto"), default="auto")
    parser.add_argument("--serial", action="append", default=[], metavar="PORT:RECEIVER_ID")
    parser.add_argument("--udp-port", type=int, default=5005, help="UDP port for wireless ESP32 CSI streaming")
    parser.add_argument("--config", type=Path, default=ROOT / "config" / "room.json")
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=8000)
    args = parser.parse_args()
    if args.source == "serial" and not args.serial:
        parser.error("--source serial requires at least one --serial PORT:RECEIVER_ID")
    import uvicorn
    uvicorn.run(create_app(args.config, args.source, args.serial, udp_port=args.udp_port), host=args.host, port=args.port)


if __name__ == "__main__":
    cli()


