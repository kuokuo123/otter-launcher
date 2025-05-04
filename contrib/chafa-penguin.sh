#!/bin/bash

# This is simple script allowing otter-launcher to print text to the left of chafa image.
# Modify $printed_lines, $image_file, $image_width, $image_height, image_padding_top, image_padding_left, text_padding to your liking, and run the script to test the printed result.

# The path of the image to be displayed
image_file="$HOME/.config/otter-launcher/prinny.png"
# Set the image's width and height, which decide the position of printed texts
image_width=30
image_height=8
# pad the image with spaces
image_padding_top=1
image_padding_left=31
# pad the text with spaces
text_padding=5

# The text to be printed should be written between the two "EOF"s
printed_lines=$(cat << EOF
┌ \x1b[1;34m  $USER@$HOSTNAME \x1b[0m───┐
│ \x1b[30m󱎘  \x1b[31m󱎘  \x1b[32m󱎘  \x1b[33m󱎘  \x1b[34m󱎘  \x1b[35m󱎘  \x1b[36m󱎘\x1b[0m │
└ \x1b[90m󱎘  \x1b[91m󱎘  \x1b[92m󱎘  \x1b[93m󱎘  \x1b[94m󱎘  \x1b[95m󱎘  \x1b[96m󱎘\x1b[0m ┘
┌ \u001B[33m \u001B[1;36m desktop\u001B[0m     $XDG_SESSION_DESKTOP ┐
│ \u001B[31m \u001B[1;36m loads\u001B[0m      $(mpstat | awk 'FNR ==4 {print $4}')% │
│ \u001B[32m \u001B[1;36m memory\u001B[0m     $(free -h | awk 'FNR == 2 {print $3}') │
EOF
)

# main function
function chafa-text() {
  # Render the image with chafa at the padded position
  printf "\033[$((image_padding_top))B"
  chafa --size $((image_width))x$((image_height)) "$image_file" | while IFS= read -r line; do
  printf "\033[$((image_padding_left))G"
    printf '\033[%dG%s\n' "$((image_padding_left))" "$line"
  done
  # Move cursor to the starting line of the image to print the text
  printf "\033[$((image_height))A"
  # pad printed_lines with spaces
  echo -e "$printed_lines" | while IFS= read -r line; do
    printf '\033[%dG%s\n' "$((text_padding))" "$line"
  done
}

# run the function
chafa-text
