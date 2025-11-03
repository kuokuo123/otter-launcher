#!/bin/bash
swaymsg [app_id=otter-launcher] resize set width 510 px height 280 px
nvim -n -c 'redraw!' '+normal G$' "$*"
swaymsg [app_id=otter-launcher] resize set width 510 px height 62 px
