#!/bin/sh

# Script for connect to a bitwarden vault, select the password you want and copy it to the clipboard
# This assume you have already connect to the bitwarden or vaultwarden vault before using one time using rbw
# Depend on fzf, wl-clipboard and rbw

clear

if ! rbw unlocked 2>/dev/null; then
    rbw unlock || { echo "Unlock failed." >&2; exit 1; }
fi

rbw sync

ITEM_NAME="$(rbw list --fields name | fzf --height 40% --reverse --header='Select an entry:')"

if PASS=$(rbw get "$ITEM_NAME" 2>/dev/null); then
    echo -n "$PASS" | wl-copy -o
    echo "Password for '$ITEM_NAME' copied to clipboard."
    rbw lock
else
    echo "Error: Could not find item '$ITEM_NAME'." >&2
    rbw lock
    exit 1
fi
