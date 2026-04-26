# WiFi and Bluetooth Sections in Quick Settings

## Overview
The quick-settings widget now includes dedicated sections for WiFi network management and Bluetooth device connectivity.

## Features Added

### 1. WiFi Network Section
- **WiFi Toggle Button** — enables/disables WiFi via `set_wifi_enabled()`
- **Network List** — displays available WiFi networks from `wifi_scan()`
- **Network Items** — shows:
  - Network name (SSID)
  - Signal strength as percentage
  - Clickable to connect via `connect(connection_id)`
- **States**:
  - "Scanning networks..." when WiFi is enabled but loading
  - "WiFi is disabled" when WiFi is turned off
  - Empty list when no networks available

### 2. Bluetooth Section
- **Bluetooth Devices List** — displays paired and connected Bluetooth devices
- **Device Items** — shows:
  - Device name
  - Device state (connected/paired)
  - Clickable to connect via `connect(device_mac)`
- **States**:
  - "No Bluetooth devices found" when no devices are available

### 3. Toggle Buttons
- **WiFi Toggle** — calls `set_wifi_enabled(enabled)` on the network interface
- **Bluetooth Toggle** — placeholder for future Bluetooth enable/disable

## Implementation Details

### State Bindings
```lua
mesh.state.set("wifi_enabled", false)
mesh.state.set("wifi_networks", {})
mesh.state.set("bluetooth_devices", {})
```

### Network Service Integration
```lua
if mesh.interfaces.get("mesh.network", ">=1.0") then
    local network_iface = mesh.interfaces.get("mesh.network", ">=1.0")
    mesh.service.on("network", "sync_network_state")
end
```

### Key Functions

#### sync_network_state()
- Fetches available WiFi networks via `wifi_scan()`
- Fetches Bluetooth devices from `devices()` list (filters by kind=="bluetooth")
- Updates state variables for template rendering

#### onToggleWiFi()
- Toggles `wifi_enabled` state
- Calls `network_iface.set_wifi_enabled(enabled)`

#### onConnectWiFi(event)
- Extracts connection ID from clicked network item (via data-id attribute)
- Calls `network_iface.connect(connection_id)`
- Logs success or error

#### onConnectBluetooth(event)
- Extracts device MAC from clicked device item (via data-id attribute)
- Calls `network_iface.connect(device_id)` with Bluetooth MAC address
- Logs success or error

### Styling
- Network sections use consistent styling with surface-container background
- Network items have hover state for interactivity
- Section titles are styled as uppercase labels
- Device/network names handle text overflow with ellipsis
- Secondary information (strength, state) displayed in muted color

## Template Structure

```mesh
<section class="network-widget">
  <div class="network-section">
    <div class="section-title">WiFi Networks</div>
    <div class="network-list">
      <!-- WiFi networks list with conditional rendering -->
    </div>
  </div>
  <div class="network-section">
    <div class="section-title">Bluetooth Devices</div>
    <div class="network-list">
      <!-- Bluetooth devices list -->
    </div>
  </div>
</section>
```

## Event Handling

### WiFi Network Connection
1. User clicks a network item
2. `onConnectWiFi` extracts the connection UUID
3. Calls `mesh.interfaces.get("mesh.network", ">=1.0").connect(uuid)`
4. Logs result (success or error with reason)

### Bluetooth Device Connection
1. User clicks a device item
2. `onConnectBluetooth` extracts the device MAC address
3. Calls `mesh.interfaces.get("mesh.network", ">=1.0").connect(mac)`
4. Backend detects MAC format and routes to `bluetoothctl connect`
5. Logs result

## Usage Flow

1. **User opens Quick Settings** (via navigation bar settings button)
2. **WiFi Section Updates** 
   - If WiFi is enabled, displays list of available networks
   - User can click to connect to a network
3. **Bluetooth Section Updates**
   - Displays all paired/connected Bluetooth devices
   - User can click to connect to a device
4. **State Synchronization**
   - Network state changes trigger `sync_network_state`
   - Lists update automatically with new networks/devices

## Dependencies
- `mesh.network` interface (v1.0 or higher)
- NetworkManager backend (nmcli for WiFi)
- BlueZ backend (bluetoothctl for Bluetooth)

## Future Enhancements
- [ ] Bluetooth enable/disable toggle (requires backend support)
- [ ] WiFi/Bluetooth connection status indicators
- [ ] Forget/Unpair network/device buttons
- [ ] WiFi password entry for new networks
- [ ] Signal strength icon rendering (instead of percentage)
- [ ] Connected/active state highlighting
- [ ] Search/filter for large device lists
