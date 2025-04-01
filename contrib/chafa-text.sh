#!/bin/bash

# This is simple script allowing otter-launcher to print text to the right of chafa image.
# Modify $printed_lines, $image_file, $image_width, $image_height, image_padding_top, image_padding_left to your liking, and run the script to test the printed result.

# The text to be printed should be written between the two "EOF"s
printed_lines=$(cat << EOF
\x1b[90m░█▀█░▀█▀░▀█▀░█▀▀░█▀█░░░░░ ░ ░
░█░█░░█░░░█░░█▀▀░█▀▄░▀▀▀░ ░ ░
░▀▀▀░░▀░░░▀░░▀▀▀░▀░▀░░░░░ ░ ░
░█░░░█▀█░█░█░█▀█░█▀▀░█░█░ ░ ░
░█░░░█▀█░█░█░█░█░█░░░█▀█░ ░ ░
░▀▀▀░▀░▀░▀▀▀░▀░▀░▀▀▀░▀░▀░ ░ ░
EOF
)
# The path of the image to be displayed
image_file="$HOME/.config/otter-launcher/otter_shocked.webp"
# Set the image's width and height, which decide the position of printed texts
image_width=17
image_height=6
# pad the image with spaces
image_padding_top=1
image_padding_left=4

# main function
function chafa-text() {
  # pad printed_lines with empty lines to the height of the image
  line_count=$(echo "$printed_lines" | wc -l)
  additional_lines=$((image_height - line_count))
  for (( i=0; i<additional_lines; i++ )); do
    printed_lines+="\n"
  done
  # Render the image with chafa at the padded position
  printf "\033[$((image_padding_top))B"
  printf "\033[$((image_padding_left))G"
  chafa --size $((image_width))x$((image_height)) "$image_file"
  # Move cursor to the starting line of the image
  printf "\033[$((image_height))A"
  # Move each of printed_lines' start position to image_width + 1
  echo -e "$printed_lines" | while IFS= read -r line; do
    printf '\033[%dG%s\n' "$((image_width + 1))" "$line"
  done
}

# run the function
chafa-text
