# otter-launcher

![Kitty Config](./assets/kitty.png)

Otter-launcher is a highly extendable commandline program to launch shell scripts, applications, or arbitrary commands by a few key strokes, customizable with ascii color codes, sixel or kitty image protocols. It is a good companion to keyboard-centric window manager users.

The concept is making the below behaviours possible:

- type "gg margaret thatcher" to google the lady
- "sh htop" to run htop in a terminal
- "dc linux" to search the word linux with an online dictionary
- "app" to launch application menu
- etc.

It is recommended to use otter-launcher with [sway-launcher-desktop](https://github.com/Biont/sway-launcher-desktop), making it a desktop application launcher. Use your wm's window rules to control its window size. 

![Demo Gif](./assets/demo.gif)

# Features

- modularized to run different commands (via configuration)
- fuzzy search and tab completion for configured modules
- vi and emacs keybindings
- per-module prehook and callback commands
- customizable shell by which programs are launched (sh -c, zsh -c, hyprctl dispatch exec, etc)
- url encoding for web searching
- decorated with ascii color codes, chafa, sixel or kitty image protocol, etc.
- minimalist, keyboard-centric

# Installation

## Building from source

```
git clone https://github.com/kuokuo123/otter-launcher /tmp/otter-launcher
cd /tmp/otter-launcher
cargo build --release
sudo cp /tmp/otter-launcher/target/release/otter-launcher /usr/bin/
```

# Configuration

Otter-launcher reads a config file from $HOME/.config/otter-launcher/config.toml. If that file is missing, it looks into /etc/otter-launcher/config.toml

An example config file is at config_example/config.toml in this repo. Copy it to one of the above locations.

Also, check [more examples of module config](https://github.com/kuokuo123/otter-launcher/wiki) at the wiki page.

``` toml
[general]
default_module = "gg" # The module to run when no prefix is matched
empty_module = "app" # run with an empty prompt
exec_cmd = "sh -c" # The exec command of your shell or window manager, default to bash
# for example: "swaymsg exec" for swaywm; "hyprctl dispatch exec" for hyprland; "zsh -c" for zsh
vi_mode = false # set true to use vi keybinds, false to use emacs keybinds; default to emacs
esc_to_abort = true # allow to quit pressing esc; a useful option for vi users


[interface]
header_cmd = "" # Run a shell command and make the stdout printed above the header
header_cmd_trimmed_lines = 0 # Remove a number of lines from header_cmd output, in case of some programs printing excessive empty lines at the end of its output
# use three quotes to write longer commands
header = """
  \u001B[32m
  ░█▀█░▀█▀░▀█▀░█▀▀░█▀█░░░░░█░░░█▀█░█░█░█▀█░█▀▀░█░█
  ░█░█░░█░░░█░░█▀▀░█▀▄░▀▀▀░█░░░█▀█░█░█░█░█░█░░░█▀█
  ░▀▀▀░░▀░░░▀░░▀▀▀░▀░▀░░░░░▀▀▀░▀░▀░▀▀▀░▀░▀░▀▀▀░▀░▀
              ————————————————————————\u001B[0m
"""
prompt_prefix = """
   \u001B[34m \u001B[0m otter-launcher \u001B[34m>\u001B[0m """
list_prefix = "      "
place_holder = "type and search..."
show_suggestion = "list" # search fro modules; autocompletion with TAB
# Three suggestion modes are avalable:
    # line (default): show suggestions at the same line as input field
    # list: list suggestions below the input field
    # none: no suggestion, and no tab completion
suggestion_lines = 4 # only take effect in the list suggestion mode
indicator_with_arg_module = "> "
indicator_no_arg_module = "< " # a sign of whether the suggested module should run with an argument
# ASCII color codes are allowed with these options, with 2 caveats:
    # 1. \x1b should be replaced with \u001B (unicode escape) because the rust toml crate cannot read \x as an escaped character...
    # 2. prefix colors are better defined in [interface].prefix_color, or the color code will compromise autocompletion function in the line suggestion mode. Not going to be fixed for a while.
prefix_color = "\u001B[33m"
description_color = "\u001B[38m"
place_holder_color = "\u001B[90m"


# Modules are defined as followed
[[modules]]
description = "search with google"
prefix = "gg"
cmd = "xdg-open 'https://www.google.com/search?q={}'"
with_argument = true # If "with_argument" is true, the {} in the cmd value will be replaced with user input. If the field is not explicitly set, will be taken as false.
url_encode = true # "url_encode" should be true if the module is set to call webpages, as this ensures special characters in url being readable to browsers. It'd better be false with shell scripts. If the field is not explicitly set, will be taken as false.

[[modules]]
description = "launch desktop applications with fzf"
prefix = "app"
prehook = "swaymsg [app_id=otter-launcher] resize set width 650 px height 300 px" # if set, the prehook command will run before the main cmd starts. 
#callback = "" # if set, the callback command will run after the main cmd has finished. 
cmd = """
desktop_file() {
find /usr/share/applications -name "*.desktop" 2>/dev/null
find /usr/local/share/applications -name "*.desktop" 2>/dev/null
find "$HOME/.local/share/applications" -name "*.desktop" 2>/dev/null
find /var/lib/flatpak/exports/share/applications -name "*.desktop" 2>/dev/null
find "$HOME/.local/share/flatpak/exports/share/applications" -name "*.desktop" 2>/dev/null
}
selected="$(desktop_file | sed 's/.desktop$//g' | sort | fzf -m -d / --with-nth -1 --reverse --padding 1,3 --prompt 'Launch Apps: ')"
[ -z "$selected" ] && exit
echo "$selected" | while read -r line ; do setsid -f gtk-launch "$(basename $line)"; done
"""

[[modules]]
description = "search github"
prefix = "gh"
cmd = "xdg-open https://github.com/search?q='{}'"
with_argument = true
url_encode = true

[[modules]]
description = "cambridge dictionary online"
prefix = "dc"
cmd = "xdg-open 'https://dictionary.cambridge.org/dictionary/english/{}'"
with_argument = true
url_encode = true

# fzf and fd are needed to run these functions
[[modules]]
description = "open files with fzf"
prefix = "fo"
cmd = "$TERM --class fzf -e sh -c 'fd --type f | fzf | xargs -r xdg-open'"

[[modules]]
description = "open folders with fzf and yazi"
prefix = "yz"
cmd = "$TERM --class yazi -e sh -c 'fd --type d | fzf | xargs -r $TERM -e yazi'"
```

# Examples for Styling

## Default Config File

![Default Config](./assets/default.png)

```
[interface]
header = """
  \u001B[32m
  ░█▀█░▀█▀░▀█▀░█▀▀░█▀█░░░░░█░░░█▀█░█░█░█▀█░█▀▀░█░█
  ░█░█░░█░░░█░░█▀▀░█▀▄░▀▀▀░█░░░█▀█░█░█░█░█░█░░░█▀█
  ░▀▀▀░░▀░░░▀░░▀▀▀░▀░▀░░░░░▀▀▀░▀░▀░▀▀▀░▀░▀░▀▀▀░▀░▀
              ————————————————————————\u001B[0m
"""
prompt_prefix = """
   \u001B[34m \u001B[0m otter-launcher \u001B[34m>\u001B[0m """
list_prefix = "      "
place_holder = "type and search..."
suggestion_lines = 3
prefix_color = "\u001B[32m"
description_color = "\u001B[38m"
place_holder_color = "\u001B[90m"
```

## Two-liner

![One-liner Config](./assets/one-liner.png)

```
[interface]
prompt_prefix = """
   \u001B[34m \u001B[0m otter-launcher \u001B[34m>\u001B[0m """
list_prefix = "   \u001B[31m #\u001B[0m "
place_holder = "type and search..."
show_suggestion = "list"
suggestion_lines = 1
prefix_color = "\u001B[33m"
description_color = "\u001B[38m"
place_holder_color = "\u001B[30m"
```

## Pfetch Integration

![Pfetch Config](./assets/pfetch.png)

```
[interface]
header_cmd = "echo ' '; pfetch"
header_cmd_trimmed_lines = 2
header = """
               \u001B[30m———\u001B[0m\u001B[31m———\u001B[0m\u001B[32m———\u001B[0m\u001B[33m———\u001B[0m\u001B[34m———\u001B[0m\u001B[35m———\u001B[0m\u001B[36m———\u001B[0m\u001B[37m———\u001B[0m
"""
prompt_prefix = " \u001B[34m \u001B[0m otter-launcher \u001B[34m>\u001B[0m"
list_prefix = "    "
place_holder = "type and search..."
show_suggestion = "list"
suggestion_lines = 1
prefix_color = "\u001B[32m"
description_color = "\u001B[38m"
place_holder_color = "\u001B[30m"
```

## Fastfetch & Krabby Integration

![Quilava Config](./assets/quilava.png)

```
[interface]
header_cmd = "fastfetch --structure break:colors:break:os:wm:kernel:uptime:packages:memory:datetime:break --key-type icon --logo-type data --logo \"$(krabby name quilava --no-title)\""
header_cmd_trimmed_lines = 1
header = ""
prompt_prefix = " \u001B[34m \u001B[0m otter-launcher \u001B[34m>\u001B[0m"
list_prefix = "    "
place_holder = "type and search..."
show_suggestion = "list"
suggestion_lines = 1
prefix_color = "\u001B[32m"
description_color = "\u001B[38m"
place_holder_color = "\u001B[30m"
```

## Chafa & Kitty Integration

![Kitty Config](./assets/kitty.png)

[Image Source: Artist Kat Corrigan & MWMO Stormwater Park](https://www.mwmo.org/learn/visit-us/exhibits/waterways-and-otterways/)

```
[interface]
header_cmd = "chafa --fit-width $HOME/.config/otter-launcher/ascii/waterways_and_otterways.jpg"
prompt_prefix = "  \u001B[34m \u001B[0m otter-launcher \u001B[34m> \u001B[0m"
list_prefix = "     "
place_holder = "type and search"
show_suggestion = "list"
suggestion_lines = 5
prefix_color = "\u001B[33m"
description_color = "\u001B[38m"
place_holder_color = "\u001B[30m"
```
