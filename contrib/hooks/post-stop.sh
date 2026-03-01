#!/bin/sh
# Blufio post-stop lifecycle hook
# Called by systemd ExecStopPost after blufio stops.
# Customize for your deployment (e.g., backup, notification).

echo "blufio: post-stop hook complete (exit code: $EXIT_STATUS)"
