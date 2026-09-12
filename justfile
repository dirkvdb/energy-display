set shell := ["bash", "-euo", "pipefail", "-c"]

esp_target := "xtensa-esp32s3-none-elf"
simulator_tap := "tap-energy"
simulator_subnet := "192.168.69.0/24"
simulator_host_address := "192.168.69.1/24"
mqtt_broker := "192.168.1.13"

firmware-lock:
    cargo generate-lockfile

firmware-format:
    cargo fmt --all

firmware-fmt:
    cargo fmt --all -- --check

firmware-check:
    cargo check -p energydisplay-firmware --target {{esp_target}} --locked -Z build-std=core,alloc

firmware-build:
    cargo build -p energydisplay-firmware --target {{esp_target}} --release --locked -Z build-std=core,alloc

firmware-flash:
    cargo run -p energydisplay-firmware --target {{esp_target}} --release --locked -Z build-std=core,alloc

firmware-validate: firmware-fmt firmware-check

monitor:
    espflash monitor --chip esp32s3 --skip-update-check

simulator-tap-up:
    sudo ip link delete {{simulator_tap}} 2>/dev/null || true
    sudo ip tuntap add dev {{simulator_tap}} mode tap user "$USER"
    sudo ip addr replace {{simulator_host_address}} dev {{simulator_tap}}
    sudo ip link set {{simulator_tap}} up
    sudo sysctl -w net.ipv4.ip_forward=1
    sudo iptables -t nat -C POSTROUTING -s {{simulator_subnet}} -d {{mqtt_broker}}/32 -j MASQUERADE 2>/dev/null || sudo iptables -t nat -A POSTROUTING -s {{simulator_subnet}} -d {{mqtt_broker}}/32 -j MASQUERADE
    sudo iptables -C FORWARD -i {{simulator_tap}} -s {{simulator_subnet}} -d {{mqtt_broker}}/32 -p tcp --dport 1883 -j ACCEPT 2>/dev/null || sudo iptables -A FORWARD -i {{simulator_tap}} -s {{simulator_subnet}} -d {{mqtt_broker}}/32 -p tcp --dport 1883 -j ACCEPT
    sudo iptables -C FORWARD -o {{simulator_tap}} -d {{simulator_subnet}} -s {{mqtt_broker}}/32 -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT 2>/dev/null || sudo iptables -A FORWARD -o {{simulator_tap}} -d {{simulator_subnet}} -s {{mqtt_broker}}/32 -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT

simulator-tap-down:
    sudo iptables -D FORWARD -o {{simulator_tap}} -d {{simulator_subnet}} -s {{mqtt_broker}}/32 -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT 2>/dev/null || true
    sudo iptables -D FORWARD -i {{simulator_tap}} -s {{simulator_subnet}} -d {{mqtt_broker}}/32 -p tcp --dport 1883 -j ACCEPT 2>/dev/null || true
    sudo iptables -t nat -D POSTROUTING -s {{simulator_subnet}} -d {{mqtt_broker}}/32 -j MASQUERADE 2>/dev/null || true
    sudo ip link delete {{simulator_tap}} 2>/dev/null || true

simulator-run:
    cargo run -p energydisplay-simulator --locked

simulator-check:
    cargo test -p dashboard-core --locked
    cargo test -p energydisplay-firmware --lib --locked
    cargo test -p energydisplay-simulator --locked

render-perf:
    cargo bench -p dashboard-core --bench render --locked

validate: firmware-validate

test: validate simulator-check

build: firmware-build

flash: firmware-flash

sim: simulator-tap-up
    trap 'just simulator-tap-down' EXIT; cargo run -p energydisplay-simulator --locked
