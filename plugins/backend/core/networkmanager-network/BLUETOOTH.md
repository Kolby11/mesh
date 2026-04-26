# Bluetooth Support in NetworkManager Backend

## Overview
The NetworkManager backend now includes full Bluetooth device enumeration and connection management alongside WiFi/network connectivity.

## Features Added

### 1. Bluetooth Device Enumeration
- `fetch_bluetooth_devices()` — queries `bluetoothctl` to list:
  - Connected Bluetooth devices (state: "connected")
  - Paired but disconnected devices (state: "paired")
- Devices are returned as Device objects with:
  - `id`: Bluetooth MAC address (format: xx:xx:xx:xx:xx:xx)
  - `name`: Device name
  - `kind`: "bluetooth"
  - `state`: "connected" or "paired"

### 2. Bluetooth Connection Control
- `connect_bluetooth(device_id)` — uses `bluetoothctl connect <MAC>` to establish a connection
- `disconnect_bluetooth(device_id)` — uses `bluetoothctl disconnect <MAC>` to drop a connection
- Both helpers return a Result object: `{ ok: true }` or `{ ok: false, error: "..." }`

### 3. Enhanced Interface Methods
- **devices()** — now returns combined list of network devices (WiFi adapters, Ethernet, etc.) and Bluetooth devices
- **connect(connection_id)** — detects whether the ID is a Bluetooth MAC or a network connection UUID:
  - MAC format (xx:xx:xx:xx:xx:xx) → calls `bluetoothctl connect`
  - UUID → calls `nmcli connection up` (existing behavior)
- **disconnect(connection_id)** — same smart detection logic
- **on_poll()** — includes Bluetooth devices in the periodic state emission

### 4. Capability Checks
- connect/disconnect operations respect the optional `service.network.control` capability
- If capability is missing, operations return `{ ok: false, error: "permission denied" }`

## Implementation Details

### MAC Address Detection
The backend identifies Bluetooth MACs using this pattern:
```lua
connection_id:match('^[%x:]+$') and #connection_id == 17
```
This checks for hex digits and colons, exactly 17 chars long (e.g., "AA:BB:CC:DD:EE:FF").

### Bluetooth CLI Tools Used
- `bluetoothctl devices Connected` — list connected devices
- `bluetoothctl devices Paired` — list paired devices
- `bluetoothctl connect <MAC>` — establish connection
- `bluetoothctl disconnect <MAC>` — drop connection

### Error Handling
- If `bluetoothctl` is unavailable or fails, the backend logs a warning and continues (doesn't crash)
- Network operations continue even if Bluetooth queries fail

## Usage Example (Frontend)

```lua
-- Get all devices (network + Bluetooth)
local devices = mesh.interfaces.get("mesh.network", ">=1.0").devices()

-- Connect to a Bluetooth device
local result = mesh.interfaces.get("mesh.network", ">=1.0").connect("AA:BB:CC:DD:EE:FF")
if result.ok then
    -- success
else
    print("Failed:", result.error)
end

-- Disconnect from Bluetooth
local result = mesh.interfaces.get("mesh.network", ">=1.0").disconnect("AA:BB:CC:DD:EE:FF")
```

## Requirements
- `bluetoothctl` installed (part of BlueZ Bluetooth stack)
- Bluetooth service running on the host
- User running the shell has permission to use `bluetoothctl`

## Limitations & Future Improvements
- **Pairing not yet automated**: The current backend assumes devices are already paired. To add pairing UI, we would need:
  - `bluetoothctl scan on/off` for discovery
  - `bluetoothctl pair <MAC>` to initiate pairing
  - Possible DBus integration for more robust Bluetooth control
- **RSSI/Signal strength**: Bluetooth devices in the Device list have no signal strength equivalent (unlike WiFi)
- **Device info**: Additional Bluetooth device properties (class, model, battery level) could be added via `bluetoothctl info <MAC>`

## Testing Checklist
- [ ] Verify `bluetoothctl devices Connected` and `bluetoothctl devices Paired` parse correctly on your system
- [ ] Test connecting to a Bluetooth device via `connect(mac)`
- [ ] Test disconnecting via `disconnect(mac)`
- [ ] Confirm devices() returns both network and Bluetooth devices
- [ ] Verify permission checks work (test without service.network.control capability)
