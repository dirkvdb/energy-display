set shell := ["bash", "-euo", "pipefail", "-c"]

esp_target := "xtensa-esp32s3-none-elf"
host_target := "x86_64-unknown-linux-gnu"

firmware-lock:
    cargo generate-lockfile

firmware-format:
    cargo fmt --all

firmware-fmt:
    cargo fmt --all -- --check

firmware-check:
    cargo check -p energydisplay-firmware --target {{esp_target}} --locked -Z build-std=core

firmware-build:
    cargo build -p energydisplay-firmware --target {{esp_target}} --release --locked -Z build-std=core

firmware-flash:
    cargo run -p energydisplay-firmware --target {{esp_target}} --release --locked -Z build-std=core

firmware-validate: firmware-fmt firmware-check

monitor:
    espflash monitor --chip esp32s3 --skip-update-check

simulator-run:
    cargo run -p energydisplay-simulator --target {{host_target}} --locked

simulator-check:
    cargo test -p energydisplay-simulator --target {{host_target}} --locked

validate: firmware-validate

test: validate simulator-check

build: firmware-build

flash: firmware-flash

sim: simulator-run
