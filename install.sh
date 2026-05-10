#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="matrix-screensaver"
BINARY_SRC="target/release/${BINARY_NAME}"
BINARY_DST="${HOME}/.local/bin/${BINARY_NAME}"
CONFIG_DIR="${HOME}/.config/matrix-screensaver"
AUTOSTART_DIR="${HOME}/.config/autostart"

echo "==> Building release binary..."
cargo build -p matrix-linux --release

echo "==> Installing binary to ${BINARY_DST}..."
install -Dm755 "${BINARY_SRC}" "${BINARY_DST}"

echo "==> Writing default config (skipped if already exists)..."
mkdir -p "${CONFIG_DIR}"
if [[ ! -f "${CONFIG_DIR}/config.toml" ]]; then
    cp config/default.toml "${CONFIG_DIR}/config.toml"
    echo "    Created ${CONFIG_DIR}/config.toml"
else
    echo "    Config already exists — not overwritten."
fi

echo "==> Installing KDE autostart entry..."
mkdir -p "${AUTOSTART_DIR}"
cat > "${AUTOSTART_DIR}/${BINARY_NAME}.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Matrix Screensaver
Comment=Matrix rain screensaver daemon
Exec=${BINARY_DST}
X-KDE-autostart-phase=2
X-KDE-autostart-after=panel
Hidden=false
EOF

echo ""
echo "Installation complete."
echo ""
echo "  Binary:    ${BINARY_DST}"
echo "  Config:    ${CONFIG_DIR}/config.toml"
echo "  Autostart: ${AUTOSTART_DIR}/${BINARY_NAME}.desktop"
echo ""
echo "Log out and back in (or run '${BINARY_DST}' manually) to start."
echo "Screensaver activates after $(grep timeout ${CONFIG_DIR}/config.toml | head -1 | grep -o '[0-9]*') seconds idle."
