#!/bin/sh
# Blufio pre-start lifecycle hook
# Called by systemd ExecStartPre before blufio starts.
# Customize for your deployment (e.g., DB backup, config check).

set -e

# Ensure data directory exists
mkdir -p /var/lib/blufio

# Optional: validate config before starting
# /usr/local/bin/blufio config validate || exit 1

echo "blufio: pre-start hook complete"
