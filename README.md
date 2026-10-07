# Deep Basics Monitor

A privacy-preserving, CCTV-style room monitor built from Wi-Fi Channel State
Information (CSI). It renders **inferred motion and rough position**, never camera
pixels or a person's appearance.

## What works now

- Browser dashboard with a top-down room view, four RF links, trail, confidence,
  motion state, signal plots, and event log.
- Built-in deterministic simulator (no hardware required).
- Serial ingestion for one or more ESP32 CSI receivers.
- Empty-room calibration and robust motion scoring.
- Rough position estimate from link disturbance weights.
- ESP-IDF firmware scaffold for an ESP32 receiver.
- Unit tests for parsing, calibration, and inference.

## What it cannot do

This does not create photographic footage. Wi-Fi CSI does not contain camera
pixels. A pose model needs a labeled dataset collected in the exact room; the
starter intentionally shows only defensible RF-derived information.

## Run the simulator

Requires Python 3.10+.

```bash
cd wifi-cctv
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
python -m app.main --source simulate --port 8000
```

Open <http://127.0.0.1:8000>. Click **Calibrate empty room** while the simulated
person is hidden, or wait for automatic warm-up.

## Run with ESP32 receiver(s)

1. Install ESP-IDF 5.x and enter its shell.
2. In `firmware/esp32-csi-receiver`, set the target matching your board:

   ```bash
   idf.py set-target esp32
   idf.py menuconfig
   ```

3. Under `Wi-Fi RF Monitor`, enter the 2.4 GHz SSID and password. Flash:

   ```bash
   idf.py -p /dev/ttyUSB0 build flash monitor
   ```

4. Stop the IDF monitor and start the server:

   ```bash
   python -m app.main --source serial \
     --serial /dev/ttyUSB0:receiver-1 \
     --serial /dev/ttyUSB1:receiver-2
   ```

5. Open the dashboard, ensure the room is empty, then click **Calibrate empty
   room**. Walk through marked locations and tune `config/room.json`.

On Linux you may need membership in `dialout`. Keep all routers/receivers fixed
after calibration. CSI is channel-specific, so a receiver associated with a
router observes that router's current channel.

## Recommended placement

Place four receivers at approximately waist/chest height around the room, not
all on one wall. Edit each link's `x`, `y` coordinate in `config/room.json` to
match the receiver or link-sensitive point. Start with one receiver and router;
add links only after the signal is stable.

## API

- `GET /api/state` latest inferred state
- `GET /api/config` public room configuration
- `POST /api/calibrate` reset and capture a fresh empty-room baseline
- `WS /ws` live updates

## Data format

Firmware emits one CSV record per CSI frame:

```text
CSI_DATA,<receiver>,<unix_ms>,<source_mac>,<rssi>,<noise_floor>,<channel>,<len>,<i0>,<q0>,...
```

The serial collector also accepts Espressif-style records where the receiver ID
is supplied by the `--serial PORT:ID` argument. Malformed lines are ignored and
counted rather than crashing the stream.

## Validation

```bash
python -m unittest discover -s tests -v
```

## Safety and privacy

Use only in spaces and on networks where every affected person has consented.
Do not present inferred locations as certain or use this as a life-safety,
security, or medical system. Keep the dashboard labeled as RF inference.

