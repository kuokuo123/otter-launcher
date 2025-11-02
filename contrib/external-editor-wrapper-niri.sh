#!/bin/bash
niri msg action set-window-height 450 && niri msg action center-window
nvim -n -c 'redraw!' '+normal G$' "$*"
niri msg action set-window-height 60 && sleep 0.05 && niri msg action center-window
exit
