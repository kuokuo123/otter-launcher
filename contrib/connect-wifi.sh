#!/bin/sh

# Allow to connect to wifi with a little fuzzy finder menu
# Depend on iwd, fzf and awk

Station="$(iwctl device list | awk '/station/ {print $2}')"

iwctl station "$Station" scan

SSID="$(iwctl station "$Station" get-networks | sed 's/\x1b\[[0-9;]*m//g' | tail -n +5 | cut -c6- | \
fzf | awk -F'  +' '{print $1}' | cut -c2-)"

echo "Connecting ..."

iwctl station "$Station" connect "$SSID"
