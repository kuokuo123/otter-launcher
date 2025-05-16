#!/bin/bash

# This is a simple script that prints a chafa image to the left of otter-launcher.
# Source Code: https://github.com/kuokuo123/otter-launcher

# ----------------------------------------------------------------------------------
# Modify variables in the fenced section to your liking

# The text to be printed (should be written between the two "EOF"s)
printed_lines=$(cat << EOF


\x1b[34;1m\x1b[0m $USER@$HOSTNAME
EOF
)
# The path of the image to be displayed
image_file="$HOME/.config/otter-launcher/image"
# width and height
image_width=24
image_height=10
# pad the image with spaces
image_padding_top=0
image_padding_left=0
# spacing between image and text
image_text_spacing=2

# ----------------------------------------------------------------------------------

# main function
function chafa-text() {
  # Render the image with chafa at the padded position
  if [ "$image_padding_top" -gt "0" ]; then
      printf "\033[$((image_padding_top))B"
  fi
  printf "%s" "$(chafa --size $((image_width))x$((image_height)) "$image_file")" | while IFS= read -r line; do
  printf "\033[$((image_padding_left))G"
    printf '\033[%dG%s' "$((image_padding_left))" "$line"
  done
  # Move cursor to the starting line of the image
  printf "\033[$((image_height - 1))A"
  # Move each of printed_lines' start position to the right
  echo -e "$printed_lines" | while IFS= read -r line; do
    printf '\033[%dG%s\n' "$((image_width + image_text_spacing))" "$line"
  done
  # move interface.header to the right
  printf "\033[$((image_width + image_text_spacing))G"
  # move cursor position to the right
  printf "%$((image_width + image_text_spacing - 4))s"
  printf "\033[$((image_width + image_text_spacing))G"
}

# run the function
chafa-text
