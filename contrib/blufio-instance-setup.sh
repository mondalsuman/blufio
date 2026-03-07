#!/usr/bin/env bash
# blufio-instance-setup.sh — Create a new Blufio instance for multi-instance systemd deployment.
#
# Usage:
#   sudo ./contrib/blufio-instance-setup.sh <instance-name> [--port PORT]
#
# Creates:
#   /etc/blufio/instances/<name>/config.toml    — Instance configuration
#   /etc/blufio/instances/<name>/environment     — Environment variables (secrets)
#   /var/lib/blufio/instances/<name>/            — Instance working directory
#   /var/lib/blufio/instances/<name>/plugins/    — WASM plugins
#   /var/lib/blufio/instances/<name>/hooks/      — Optional lifecycle hooks
#
# After setup:
#   1. Edit /etc/blufio/instances/<name>/config.toml (set adapters, providers)
#   2. Edit /etc/blufio/instances/<name>/environment (set API keys)
#   3. systemctl enable --now blufio@<name>

set -euo pipefail

INSTANCE_NAME="${1:-}"
PORT=""

# Parse remaining arguments for --port flag.
shift || true
while [[ $# -gt 0 ]]; do
    case "$1" in
        --port)
            PORT="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

if [[ -z "$INSTANCE_NAME" ]]; then
    echo "Usage: $0 <instance-name> [--port PORT]"
    echo ""
    echo "Examples:"
    echo "  $0 personal"
    echo "  $0 work --port 3001"
    echo "  $0 team --port 3002"
    exit 1
fi

# Validate instance name (alphanumeric, hyphens, underscores).
if ! [[ "$INSTANCE_NAME" =~ ^[a-zA-Z0-9_-]+$ ]]; then
    echo "Error: instance name must be alphanumeric (with hyphens/underscores)" >&2
    exit 1
fi

CONFIG_DIR="/etc/blufio/instances/${INSTANCE_NAME}"
DATA_DIR="/var/lib/blufio/instances/${INSTANCE_NAME}"
DEFAULT_PORT="${PORT:-3000}"

# Check if instance already exists.
if [[ -d "$CONFIG_DIR" ]] || [[ -d "$DATA_DIR" ]]; then
    echo "Error: instance '${INSTANCE_NAME}' already exists" >&2
    echo "  Config: ${CONFIG_DIR}"
    echo "  Data:   ${DATA_DIR}"
    exit 1
fi

echo "Creating Blufio instance: ${INSTANCE_NAME}"
echo "  Config: ${CONFIG_DIR}"
echo "  Data:   ${DATA_DIR}"
echo "  Port:   ${DEFAULT_PORT}"
echo ""

# Create directories.
mkdir -p "${CONFIG_DIR}"
mkdir -p "${DATA_DIR}/plugins"
mkdir -p "${DATA_DIR}/hooks"

# Create default config.toml with instance-specific port.
cat > "${CONFIG_DIR}/config.toml" << TOML
# Blufio instance configuration: ${INSTANCE_NAME}
# See deploy/.env.example for environment variable reference.

[agent]
name = "blufio-${INSTANCE_NAME}"

[gateway]
enabled = true
host = "127.0.0.1"
port = ${DEFAULT_PORT}

[storage]
database_path = "${DATA_DIR}/blufio.db"

[plugin]
skill_dir = "${DATA_DIR}/plugins"
TOML

# Create environment file template.
cat > "${CONFIG_DIR}/environment" << ENV
# Environment variables for blufio@${INSTANCE_NAME}
# Loaded by systemd EnvironmentFile= directive.
#
# Uncomment and set your API keys:
# ANTHROPIC_API_KEY=sk-ant-...
# OPENAI_API_KEY=sk-...
# TELEGRAM_BOT_TOKEN=...
# DISCORD_TOKEN=...
RUST_LOG=blufio=info
ENV

# Set ownership (create blufio user if needed).
if id blufio &>/dev/null; then
    chown -R blufio:blufio "${DATA_DIR}"
    chown -R root:blufio "${CONFIG_DIR}"
    chmod 750 "${CONFIG_DIR}"
    chmod 640 "${CONFIG_DIR}/environment"
else
    echo "Warning: 'blufio' user does not exist. Run:"
    echo "  useradd --system --shell /usr/sbin/nologin --home-dir /var/lib/blufio blufio"
fi

echo ""
echo "Instance '${INSTANCE_NAME}' created successfully."
echo ""
echo "Next steps:"
echo "  1. Edit ${CONFIG_DIR}/config.toml"
echo "  2. Edit ${CONFIG_DIR}/environment (add API keys)"
echo "  3. systemctl enable --now blufio@${INSTANCE_NAME}"
echo ""
echo "Management:"
echo "  systemctl status blufio@${INSTANCE_NAME}"
echo "  journalctl -u blufio@${INSTANCE_NAME} -f"
