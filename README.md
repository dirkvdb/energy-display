# Energy Display

Bare-metal Rust port of `../inky-solar` for the Waveshare ESP32-S3-RLCD-4.2. Board support is validated, and the firmware renders the source application's Advanced dashboard from a shared `no_std` model and renderer. It now connects to Wi-Fi with Embassy, receives live MQTT v5 telemetry through a bounded `rust-mqtt` client, and redraws the display for each accepted update.

See [`PORTING_PLAN.md`](PORTING_PLAN.md) for the remaining clock, retained-summary, and reliability work.

## Current hardware target

| Function | Setting |
|---|---|
| MCU/module | ESP32-S3-WROOM-1-N16R8 |
| Display | ST7305, native 300×400 |
| Application orientation | 400×300 landscape |
| SPI | SPI2, mode 0, MSB first, 10 MHz |
| SCLK / MOSI | GPIO11 / GPIO12 |
| D/C / CS / RESET | GPIO5 / GPIO40 / GPIO41 |
| TE | GPIO6, intentionally unused |

The host simulator renders the same deterministic dashboard fixture used by `inkytool test` in `../inky-solar`. The firmware starts with an empty dashboard, preserves the most recently received state across network outages, and redraws immediately after each valid MQTT update. The status-bar date remains a temporary monotonic fixture until RTC/SNTP support is added.

The serial console logs display startup, Wi-Fi connection and reconnect state, the DHCP address, MQTT connection/subscription state, payload rejection details, display updates, and `heartbeat: dashboard displayed` every five seconds.

## Development environment

All Cargo, build, and flash operations must run through [`devenv`](https://devenv.sh/). `devenv.nix` provides `espup`, `espflash`, and `rustup`, and installs Espressif Rust `1.97.0.0` into the ignored `.devenv` state directory. The first command can take a while while that toolchain downloads.

### Secrets

[`secretspec.toml`](secretspec.toml) declares three required build-time secrets. Store them in the configured OS keyring without putting values on the command line:

```sh
secretspec set --provider keyring WIFI_SSID
secretspec set --provider keyring WIFI_PASSWORD
secretspec set --provider keyring MQTT_PASSWORD
```

Each command prompts for its value. `devenv` resolves the `default` Secretspec profile and exports the values while compiling. The firmware uses `env!`, so all three credentials are embedded in the flashed binary; anyone able to read the firmware image may recover them. They are never printed by the firmware.

The non-secret MQTT defaults in `firmware/src/config.rs` preserve the existing deployment contract: broker `192.168.1.13:1883`, username `iot`, client ID `inkydisplay`, MQTT v5, and a 180-second keepalive. Port 1883 is unencrypted, so credentials and telemetry are exposed to observers on the local network.

Enter the environment:

```sh
devenv shell
```

Or run the checked-in tasks directly:

| Recipe | Purpose |
|---|---|
| `just build` | Build the optimized release firmware |
| `just test` | Validate firmware and test the dashboard core and simulator |
| `just sim` | Show the firmware renderer in a local window |

| Command | Purpose |
|---|---|

| `just firmware-format` | Apply Rust formatting |
| `just validate` | Check formatting and type-check the target |
| `just build` | Produce the optimized release ELF |
| `just flash` | Build and flash without opening a monitor |
| `just monitor` | Open the interactive serial monitor |
| `just firmware-lock` | Refresh `Cargo.lock` after dependency changes |
| `just sim` | Show the firmware renderer in a local window |
| `just simulator-check` | Build and test the native simulator |

Do not run an ambient `cargo` directly; it may select the wrong compiler because ESP32-S3 requires Espressif's Xtensa Rust toolchain.

## Local simulator

Run the display renderer without connecting a board:

```sh
just sim
```

Run `devenv shell` first; it provisions and activates the pinned Espressif toolchain automatically, then use the `just` commands above.

The simulator opens a 2x-scale window with the same deterministic 400x300 Advanced dashboard frame produced by the firmware. Close the window or press Escape to stop it. The simulator builds for the development machine while the normal firmware tasks continue to build for ESP32-S3.

This is a display-level simulator rather than an ESP32 instruction emulator. It executes the exact shared `embedded-graphics` drawing code, but does not emulate SPI, the ST7305 controller initialization, Embassy timing, or other peripherals.

## Flash and hardware check

Connect the board over USB, then run:

```sh
just flash
just monitor
```

`just flash` builds the release image and flashes it. The separate monitor command is intentionally interactive and runs until stopped.

Acceptance checks:

1. The serial log reaches `display: empty dashboard rendered` without a panic.
2. Wi-Fi reports `wifi: connected`, followed by `network: DHCP address ...`.
3. MQTT reports a TCP connection and eight successful subscription acknowledgements, followed by `mqtt: subscribed to 8 live filters ...`.
4. Published telemetry produces `display: dashboard updated` and appears in the corresponding dashboard fields.
5. Disconnecting the access point produces Wi-Fi/network/MQTT failure logs while the last dashboard frame remains visible; restoring it reconnects and resubscribes.
6. The complete outer border is visible and stable, and `heartbeat: dashboard displayed` continues every five seconds.

Text rendering now matches the source path: the OFL-licensed Bitter Pro Black font is shaped/rasterized at runtime by `cosmic-text` 0.19 with its `no_std` and `swash` features. During environment construction, `devenv.nix` subsets the checked-in OTF to printable ASCII plus `°`, `↑`, and `↓`, preserving the original family and Black weight metadata before embedding it in flash. The renderer preserves the source font sizes, advanced shaping, `alpha > 127` monochrome threshold, alignment, and ink-bound vertical centering. The source Font Awesome asset is a Pro font without a checked-in redistribution license, so it is not copied. Instead, `devenv.nix` takes the Apache-2.0 `material-design-icons` font from Nixpkgs and subsets it to the five required glyphs. The grid, solar, heating, shower, and center backup-heater icons use `lightning-bolt`, `solar-power-variant-outline`, `heating-coil`, `shower-head`, and `recycle-variant`, respectively. The small derived font is embedded in the firmware; the remaining monochrome symbols continue to use the existing local geometry.

If the panel remains blank or is unstable, keep SPI at 10 MHz and compare the initialization sequence with `.board-reference` before changing frequencies. The pinned `st7305` crate intentionally gets tested unchanged first; the vendor example includes an additional gate-timing command (`0x62`) that may require an upstream driver fix if hardware proves it necessary.

## Project layout

```text
.
├── .cargo/config.toml       # Xtensa target, linker flags, espflash runner
├── assets/fonts/            # Font assets and third-party license notices
├── crates/
│   └── dashboard-core/      # no_std model, MQTT decoding, formatting, renderer
├── firmware/
│   └── src/
│       ├── board.rs         # Board dimensions, pins, and memory settings
│       ├── config.rs        # Embedded credentials and MQTT defaults
│       ├── display.rs       # ST7305 clear/polarity helpers
│       ├── tasks/
│       │   ├── mqtt.rs      # Bounded MQTT v5 client and typed update channel
│       │   └── net.rs       # Wi-Fi reconnect, DHCP status, and network runner
│       ├── lib.rs           # Shared board/display adapter
│       └── main.rs          # Runtime, hardware initialization, and display owner
├── simulator/
│   └── src/main.rs          # Native window using the shared renderer
├── Cargo.toml               # Workspace
├── devenv.nix               # Canonical toolchain and task definitions
└── PORTING_PLAN.md          # Full dashboard roadmap
```
