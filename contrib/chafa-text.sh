#!/bin/bash

#
# This is a simple script that prints a chafa image to the left of otter-launcher.
# Source Code: https://github.com/kuokuo123/otter-launcher
#

# ----------------------------------------------------------------------------------
# Modify variables in the fenced section to your liking

# The text to be printed (should be written between the two "EOF"s)
printed_lines=$(cat << EOF

\x1b[34;1m\x1b[0m $USER@$HOSTNAME    \u001B[35m󰍛\u001B[0m $(free -h | awk 'FNR == 2 {print $3}')
EOF
)
# The path of the image to be displayed
image_file="$HOME/.config/otter-launcher/nu-gundam.png"
# width and height
image_width=19
image_height=8
# pad the image with spaces
image_padding_top=1
image_padding_left=2
# spacing between image and text
image_text_spacing=1

# ----------------------------------------------------------------------------------

# main function
function chafa-text() {
  # Render the image with chafa at the padded position
  printf "\033[$((image_padding_top))B"
  chafa --size $((image_width))x$((image_height)) "$image_file" | while IFS= read -r line; do
  printf "\033[$((image_padding_left))G"
    printf '\033[%dG%s\n' "$((image_padding_left))" "$line"
  done
  # Move cursor to the starting line of the image
  printf "\033[$((image_height + 1))A"
  # Move each of printed_lines' start position to image_width + 1
  echo -e "$printed_lines" | while IFS= read -r line; do
    printf '\033[B\033[%dG%s' "$((image_width + image_text_spacing))" "$line"
  done
  # start a new line to render interface.header
  printf "\n"
  # move interface.header to image_width+1
  printf "\033[$((image_width + image_text_spacing))G"
  # move cursor position to image_width+1
  printf "%$((image_width + image_text_spacing - 1))s"
  printf "\033[$((image_width + image_text_spacing))G"
}

# run the function
chafa-text
