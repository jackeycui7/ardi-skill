#!/usr/bin/env bash
# install.sh — set up Ardi auto-mining as a systemd user service.
#
# What this does:
#   1. Symlink this directory into a stable path under ~/.local/share/.
#   2. Create ~/.ardi-agent/auto-mine.env from the template if missing
#      (and remind the user to edit it before starting).
#   3. Install systemd USER units (so it doesn't need root). The timer
#      runs as long as the user is logged in OR linger is enabled.
#   4. Enable the timer (without starting it — user must edit env first).
#
# After install: edit ~/.ardi-agent/auto-mine.env, then:
#   systemctl --user start ardi-mine.timer
#   journalctl --user -u ardi-mine -f

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
INSTALL_DIR="$HOME/.local/share/ardi-auto-mine"
ENV_FILE="$HOME/.ardi-agent/auto-mine.env"
SYSTEMD_USER_DIR="$HOME/.config/systemd/user"

say() { printf "\033[36m▸ %s\033[0m\n" "$*"; }
warn() { printf "\033[33m⚠ %s\033[0m\n" "$*"; }

# 1. Stable symlink
say "Linking $HERE → $INSTALL_DIR"
mkdir -p "$(dirname "$INSTALL_DIR")"
ln -snf "$HERE" "$INSTALL_DIR"

# 2. Create env file
say "Setting up env at $ENV_FILE"
mkdir -p "$(dirname "$ENV_FILE")"
if [[ ! -f "$ENV_FILE" ]]; then
  cp "$HERE/config.example.env" "$ENV_FILE"
  chmod 600 "$ENV_FILE"
  warn "Edit $ENV_FILE before starting the timer (set ANTHROPIC_API_KEY etc)"
else
  say "env already exists; not overwriting"
fi

# 3. Install systemd user units
say "Installing systemd user units to $SYSTEMD_USER_DIR"
mkdir -p "$SYSTEMD_USER_DIR"

# Patch the service unit to point at our install path.
sed "s|%h/ardi-skill/tools/auto-mine|$INSTALL_DIR|g" \
  "$HERE/systemd/ardi-mine.service" > "$SYSTEMD_USER_DIR/ardi-mine.service"
cp "$HERE/systemd/ardi-mine.timer" "$SYSTEMD_USER_DIR/ardi-mine.timer"

systemctl --user daemon-reload

# 4. Enable (don't start yet — user must finish env first)
say "Enabling timer (will start on next boot OR when you 'systemctl --user start ardi-mine.timer')"
systemctl --user enable ardi-mine.timer

# Optionally enable lingering so the timer runs even when the user is
# logged out. Skip if running over ssh as root.
if command -v loginctl >/dev/null && [[ "$(id -u)" -ne 0 ]]; then
  if ! loginctl show-user "$USER" --property=Linger | grep -q "Linger=yes"; then
    warn "User lingering is OFF. To run when you're not logged in:"
    echo "    sudo loginctl enable-linger $USER"
  fi
fi

cat <<EOF

╭─────────────────────────────────────────────────────────╮
│  ✓ Installed.                                            │
│                                                          │
│  Next steps:                                             │
│    1. Edit $ENV_FILE                                     │
│       (at minimum: ANTHROPIC_API_KEY)                    │
│    2. Make sure 'ardi-agent' and your chosen runtime    │
│       (claude / hermes / openclaw) are on PATH           │
│    3. Run a dry tick to verify wiring:                   │
│         $INSTALL_DIR/ardi-tick.sh                        │
│    4. Start the timer:                                   │
│         systemctl --user start ardi-mine.timer           │
│    5. Watch it work:                                     │
│         journalctl --user -u ardi-mine -f                │
│                                                          │
│  To stop:                                                │
│    systemctl --user stop ardi-mine.timer                 │
│  To uninstall:                                           │
│    systemctl --user disable --now ardi-mine.timer        │
│    rm $SYSTEMD_USER_DIR/ardi-mine.{service,timer}        │
│                                                          │
╰─────────────────────────────────────────────────────────╯
EOF
