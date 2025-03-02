# Otter Launcher

![Default Config](./assets/default.png)

Otter-launcher is a highly extendable commandline program that can launch shell scripts, applications, or arbitrary commands by a few key strokes. It is customizable with ascii color code, sixel or kitty image protocol (depending on the emulator in use), and hence a good companion to keyboard-centric window manager users.

The concept is making behaviours like the below possible:

- type "gg margaret thatcher" to google the lady
- type "sh htop" to run htop in a terminal
- type "dc linux" to search the word linux with an online dictionary
- type "app" to launch sway-launcher-desktop
- etc.

It is recommended to use otter-launcher with [sway-launcher-desktop](https://github.com/Biont/sway-launcher-desktop), making it an application launcher. Use your wm's window rules to control its window size. 

![Demo Gif](./assets/demo.gif)

![Demo-2 Gif](./assets/demo-2.gif)

# Features

- modularized prefixes to run different commands (via configuration)
- fuzzy search and tab completion for configured modules
- module-specific prehook and callback commands
- customizable shell or wm by which programs are launched (sh -c, zsh -c, hyprctl dispatch exec, etc)
- url encoding for web searching
- customizable with ascii color codes, chafa, neofetch, etc.
- minimalist, keyboard-centric design

# Installation

## Building from source

```
git clone https://github.com/kuokuo123/otter-launcher /tmp/otter-launcher
cd /tmp/otter-launcher
cargo build --release
sudo cp /tmp/otter-launcher/target/release/otter-launcher /usr/bin/
```

# Configuration

Otter-launcher read a config file from $HOME/.config/otter-launcher/config.toml. If that file is missing, it looks into /etc/otter-launcher/config.toml

``` toml
[general]
# The module to run when no prefix is matched
default_module = "gg"
# The module to run with an empty prompt
empty_module = ""
# Your shell or window manager, default to sh
# for example: "swaymsg exec" for swaywm; "hyprctl dispatch exec" for hyprland; "zsh -c" for zsh
exec_cmd = "sh -c"
# Fuzzy search for prefixes; autocompletion with TAB
show_suggestion = true


[interface]
# Ascii color codes are allowed. However, \x1b[ should be replaced with \u001B[ (unicode escape) because the rust toml crate cannot read \x as an escaped character...
header_cmd = "" # Run a shell command with its output printed as the header
header_cmd_trimmed_lines = 0 # Remove a number of lines from header_cmd output, in case of excessive empty lines printed at the end
header = "  \u001B[34m \u001B[0m Search"
prompt_prefix = "\u001B[34m>\u001B[0m"
list_prefix = "    "
highlighted_prefix = "  \u001B[34m >\u001B[0m"
scroll_up_prefix = "  \u001B[34m ^\u001B[0m"
scroll_down_prefix = "  \u001B[34m v\u001B[0m"
help_message = ""
place_holder = "type and search..."
suggestion_lines = 1


# Modules are defined as below. Desc, prefix, and cmd are essential and must be specified; others are optional.
[[modules]]
description = "search with google"
prefix = "\u001B[32mgg\u001B[0m"
cmd = "xdg-open 'https://www.google.com/search?q={}'"
# If "with_argument" is true, the {} in the cmd value will be replaced with user input. If the field is not explicitly set, will be taken as false.
with_argument = true
# "url_encode" should be true if the module is set to call webpages, as this ensures special characters in url being readable to browsers. It'd better be false with shell scripts. If the field is not explicitly set, will be taken as false.
url_encode = true

[[modules]]
description = "open files with fzf"
prefix = "\u001B[32mfo\u001B[0m"
cmd = "$TERM --class fzf -e sh -c 'fd --type f | fzf | xargs -r xdg-open'"
# if set, the prehook command will run before the main cmd starts. 
prehook = "swaymsg '[app_id=fzf] floating on; [app_id=fzf] resize set width 600 px height 300 px'"
# if set, the callback command will run after the main cmd has finished. 
callback = ""

[[modules]]
description = "search for directories with yazi"
prefix = "\u001B[32myz\u001B[0m"
cmd = "$TERM --class yazi -e sh -c 'fd --type d | fzf | xargs -r $TERM -e yazi'"

[[modules]]
description = "cambridge dictionary online"
prefix = "\u001B[32mdc\u001B[0m"
cmd = "xdg-open 'https://dictionary.cambridge.org/dictionary/english/{}'"
with_argument = true
url_encode = true
```

# Examples for Styling

## Default

![Default Config](./assets/default.png)

```
header_cmd = ""
header_cmd_trimmed_lines = 0
header = "  \u001B[34m \u001B[0m Search"
prompt_prefix = "\u001B[34m>\u001B[0m"
list_prefix = "    "
highlighted_prefix = "  \u001B[34m >\u001B[0m"
scroll_up_prefix = "  \u001B[34m ^\u001B[0m"
scroll_down_prefix = "  \u001B[34m v\u001B[0m"
help_message = ""
place_holder = "type and search..."
suggestion_lines = 1
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
highlighted_prefix = "    "
scroll_up_prefix = "    "
scroll_down_prefix = "    "
help_message = ""
place_holder = "type and search..."
suggestion_lines = 1
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
highlighted_prefix = "    "
scroll_up_prefix = "    "
scroll_down_prefix = "    "
help_message = ""
place_holder = "type and search..."
suggestion_lines = 1
```

## Kitty + Uwufetch

![Kitty Config](./assets/kitty.png)

```
[interface]
header_cmd = "uwufetch -i otter.png"
header_cmd_trimmed_lines = 3
header = """
               \u001B[30m———\u001B[0m\u001B[31m———\u001B[0m\u001B[32m———\u001B[0m\u001B[33m———\u001B[0m\u001B[34m———\u001B[0m\u001B[35m———\u001B[0m\u001B[36m———\u001B[0m\u001B[37m———\u001B[0m
"""
prompt_prefix = " \u001B[34m \u001B[0m otter-launcher \u001B[34m>\u001B[0m"
list_prefix = "    "
highlighted_prefix = "    "
scroll_up_prefix = "    "
scroll_down_prefix = "    "
help_message = ""
place_holder = "type and search..."
suggestion_lines = 1
```
