# Energy Display

Bare-metal Rust port of `../inky-solar` for the Waveshare ESP32-S3-RLCD-4.2. Board support is validated, and the firmware renders the source application's Advanced dashboard from a shared `no_std` model and renderer. It connects to Wi-Fi with Embassy, receives live MQTT v5 telemetry through a bounded `rust-mqtt` client, supports authenticated over-the-air firmware updates, and redraws the display for each accepted update.

See [`PORTING_PLAN.md`](PORTING_PLAN.md) for the remaining RTC, retained-summary, and reliability work.

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
| Buttons | BOOT GPIO0 and KEY GPIO18, active low |

The host simulator tests against the same deterministic dashboard fixture used by `inkytool test` in `../inky-solar`. Its live mode starts with an empty dashboard, preserves the most recently received state across network outages, and redraws immediately after each valid MQTT update, just like the firmware. On hardware, SNTP synchronizes UTC with `192.168.1.1` and a monotonic-backed software clock presents `Europe/Brussels` local time with CET/CEST transitions. Until the first successful synchronization, the status bar shows `Waiting for time` and hourly aggregation remains uninitialized.

The serial console unconditionally logs display startup, heap usage, Wi-Fi connection and reconnect state, the DHCP address, NTP synchronization, MQTT connection/subscription state, and payload rejection details. A TIMG0 hardware watchdog resets the system if the Embassy executor fails to feed it for 10 seconds; normal operation feeds it once per second. Warning and error `log` records are also sent asynchronously as structured JSON Lines to Victoria Logs at `192.168.1.13:9428`. If that server is unavailable, serial logging continues and the one-record forwarding queue drops excess records rather than blocking firmware tasks. Panics are checksummed into the dedicated `panic` flash partition before a software reset. On the next boot, the recovered message is queued as a structured error and retained in flash until Victoria Logs accepts it.

Application code performs no heap allocation after boot: renderer glyphs are generated at build time and embedded as static 1-bit bitmaps, while MQTT, HTTP logging, OTA, SNTP, Embassy sockets, channels, strings, and the display framebuffer use fixed-capacity or static storage. The former 165 KiB runtime font heap and the logger's lazy boxed future/buffer have been removed. A guarded Rust global allocator is restricted before long-lived tasks start: small allocations up to 1 KiB remain available for `esp-rtos` timer and synchronization bookkeeping, while larger allocations are rejected and identified in serial and persisted panic messages. The only backing heap is a 64 KiB reclaimed-RAM region also required by Espressif's closed-source Wi-Fi driver. That driver cannot operate allocation-free: its C ABI bypasses the Rust per-allocation limit and always creates dynamic RX packet copies, while this ESP32-S3 binary is compiled for dynamic TX copies. Firmware bounds those vendor allocations to eight RX and eight TX packets, uses four static hardware RX buffers, and disables AMPDU reorder state. Connect/disconnect logs report driver-heap usage so reconnect tests can detect retained state.

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

The non-secret MQTT defaults in `firmware/src/config.rs` preserve the existing deployment contract: broker `192.168.1.13:1883`, username `iot`, client ID `energydisplay`, MQTT v5, and a 180-second keepalive. Port 1883 is unencrypted, so credentials and telemetry are exposed to observers on the local network.

Enter the environment:

```sh
devenv shell
```

Or run the checked-in tasks directly:

| Recipe | Purpose |
|---|---|
| `just build` | Build the optimized release firmware |
| `just test` | Validate firmware and test the dashboard core and simulator |
| `just sim` | Configure TAP/NAT with `sudo` and run the live simulator |

| Command | Purpose |
|---|---|

| `just firmware-format` | Apply Rust formatting |
| `just validate` | Check formatting and type-check the target |
| `just build` | Produce the optimized release ELF |
| `just flash` | Build, flash, and open the interactive serial monitor |
| `just ota-push <address>` | Build and push firmware to a running device over Wi-Fi |
| `just monitor` | Open the interactive serial monitor without flashing |
| `just firmware-lock` | Refresh `Cargo.lock` after dependency changes |
| `just sim` | Configure TAP/NAT with `sudo` and run the live simulator |
| `just simulator-tap-up` | Create the TAP device and restricted MQTT forwarding rules |
| `just simulator-tap-down` | Remove the TAP device and forwarding rules |
| `just simulator-run` | Run against an already configured `tap-energy` device |
| `just simulator-check` | Build and test the native simulator without TAP or root |
| `just render-perf` | Benchmark rendering against an ST7305-style host framebuffer |

Do not run an ambient `cargo` directly; it may select the wrong compiler because ESP32-S3 requires Espressif's Xtensa Rust toolchain.

## Local simulator

Run the live firmware application without connecting a board:

```sh
devenv shell
just sim
```

`just sim` asks for `sudo` to create `tap-energy`, gives the invoking user access to it, assigns the host gateway `192.168.69.1/24`, enables IPv4 forwarding, and installs narrowly scoped NAT/firewall rules allowing guest `192.168.69.2` to reach only MQTT at `192.168.1.13:1883`. Cargo and the SDL window run as the normal user. Closing the window or pressing Escape removes the TAP device and those firewall rules. The recipe leaves the system-wide `net.ipv4.ip_forward` setting enabled because it cannot safely know whether another service already depended on it.

`just render-perf` runs the shared renderer in Cargo's optimized bench profile. It reports cold and steady-state frame times together with pixel and `DrawTarget::draw_iter` call counts; set `RENDER_BENCH_SAMPLES` to override the default 50 steady-state frames.

For manual lifecycle control, use `just simulator-tap-up`, `just simulator-run`, and `just simulator-tap-down`. The teardown recipe is idempotent. `just simulator-check` remains non-privileged and does not open TAP or SDL.

The simulator opens a 2x-scale 400x300 Advanced dashboard, connects to the real broker with the embedded Secretspec MQTT password, subscribes through the same bounded `rust-mqtt` v5 session code, applies the same typed updates, and runs the same persistent renderer under Embassy's host executor. It uses the dedicated MQTT client ID `energydisplay-simulator`, so it can run alongside firmware without either client disconnecting the other.

This is a host-driver substitution rather than an ESP32 instruction emulator. Linux TAP replaces the ESP Wi-Fi link driver, while `embassy-net`, Embassy timing/tasks, MQTT parsing, bounded update channel, dashboard state, and `embedded-graphics` renderer are shared with firmware. It does not emulate Wi-Fi association, SPI, or ST7305 controller initialization.

## Flash and hardware check

Connect the board over USB, then run:

```sh
just flash
just monitor
```

`just flash` builds the release image and flashes it. The separate monitor command is intentionally interactive and runs until stopped.

The first OTA-capable installation must be made over USB with `just flash`; this installs the new factory/OTA partition table. Subsequent releases can be pushed over Wi-Fi:

```sh
just ota-push 192.168.1.42
```

The device listens on TCP port 3232. The upload metadata is authenticated with HMAC-SHA256 using the existing `MQTT_PASSWORD`, and the complete image is SHA-256 checked before the inactive OTA slot is activated. Keep the password stable across releases so a running device can authenticate the next image. The OTA transport is not encrypted, although the firmware image itself contains the same build-time credentials and should already be treated as sensitive. A successful upload reboots automatically; an interrupted or invalid upload leaves the currently running slot selected.

Acceptance checks:

1. The serial log reaches `display: empty dashboard rendered` without a panic.
2. Wi-Fi reports `wifi: connected`, followed by `network: DHCP address ...`.
3. MQTT reports a TCP connection and eight successful subscription acknowledgements, followed by `mqtt: subscribed to 8 live filters ...`.
4. Published telemetry produces `display: dashboard updated` and appears in the corresponding dashboard fields.
5. Disconnecting the access point produces Wi-Fi/network/MQTT failure logs while the last dashboard frame remains visible; restoring it reconnects and resubscribes.
6. Repeating Wi-Fi disconnect/reconnect cycles returns `wifi: driver heap after disconnect` to a stable baseline and never reports an allocation failure.
7. The complete outer border is visible and stable, and `heartbeat: dashboard displayed` continues every five seconds.

Text uses the OFL-licensed Bitter Black font. During compilation, `dashboard-core/build.rs` uses `cosmic-text` 0.19 and Swash on the host to shape and rasterize the exact supported character/icon repertoire at every dashboard size. It emits static 1-bit glyph bitmaps, four horizontal subpixel variants, advances, kerning overrides, and baseline metrics into `OUT_DIR`; `cosmic-text` and the source fonts are not runtime firmware dependencies. The runtime renderer uses only fixed-capacity `heapless` layout vectors and static flash data while preserving advanced shaping results, `alpha > 127` monochrome thresholding, wrapping, alignment, and ink-bound vertical centering. The generated bitmap payload is about 44 KiB. The source Font Awesome asset is a Pro font without a checked-in redistribution license, so it is not copied. Instead, `devenv.nix` takes the Apache-2.0 `material-design-icons` font from Nixpkgs and subsets it to the five required glyphs. The grid, solar, heating, shower, and center backup-heater icons use `lightning-bolt`, `solar-power-variant-outline`, `heating-coil`, `shower-head`, and `recycle-variant`, respectively. The remaining monochrome symbols continue to use existing local geometry.

If the panel remains blank or is unstable, keep SPI at 10 MHz and compare the initialization sequence with `.board-reference` before changing frequencies. The pinned `st7305` crate intentionally gets tested unchanged first; the vendor example includes an additional gate-timing command (`0x62`) that may require an upstream driver fix if hardware proves it necessary.

## Project layout

```text
.
├── .cargo/config.toml       # Xtensa target, linker flags, espflash runner
├── assets/fonts/            # Font assets and third-party license notices
├── crates/
│   └── dashboard-core/      # no_std model, MQTT decoding, formatting, renderer
├── firmware/
│   ├── partitions.csv        # Factory, A/B OTA, and persistent panic partitions
│   └── src/
│       ├── board.rs         # Board dimensions, pins, and memory settings
│       ├── config.rs        # Embedded credentials and MQTT defaults
│       ├── display.rs       # ST7305 clear/polarity helpers
│       ├── logging.rs       # Serial + structured JSON fan-out logger
│       ├── panic_store.rs   # Flash-backed panic handler and record codec
│       ├── tasks/
│       │   ├── mqtt.rs      # Bounded MQTT v5 client and typed update channel
│       │   ├── net.rs       # Wi-Fi reconnect, DHCP status, and network runner
│       │   ├── ota.rs       # Authenticated OTA receiver and A/B slot activation
│       │   └── structured_log.rs # Victoria Logs HTTP forwarder
│       ├── lib.rs           # Shared board/display adapter
│       └── main.rs          # Runtime, hardware initialization, and display owner
├── simulator/
│   └── src/main.rs          # Native window using the shared renderer
├── Cargo.toml               # Workspace
├── devenv.nix               # Canonical toolchain and task definitions
└── PORTING_PLAN.md          # Full dashboard roadmap
```
