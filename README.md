
# otter-launcher

![cover_pic](./assets/soothing.png)

A very hackable app launcher, designed for keyboard-centric wm users. It is blazingly fast, supports vi and emacs keybinds, and can be decorated with ascii color codes, sixel or kitty image protocols. Plus, through bash scripting, system info widgets can be added to the infinity.

The core concept is making these behaviours possible:

- type "gg margaret thatcher" to google the lady in a web browser
- "sh htop" to run htop in a terminal
- "dc linux" to search the word linux with an online dictionary
- "app" to launch application menu
- etc.

Some helper scripts can be found in the [contrib](https://github.com/kuokuo123/otter-launcher/tree/main/contrib) folder. Also, it's recommended to setup [sway-launcher-desktop](https://github.com/Biont/sway-launcher-desktop) as a module to launch desktop apps. Use your wm's window rules to control its window size.

# Demo

Workflow

![Demo Gif](./assets/demo.gif)

External Editor & List Selection

![Menu Demo](./assets/demo_menu.gif)

# Features

- modularized to run different commands
- vi and emacs keybinds
- two suggestion modes: list & hint
- tab completion; tab again to undo completion
- edit prompt in an external editor (vim, emacs, etc.)
- url encoding for web searching
- supporting ascii color codes, chafa, sixel or kitty image protocol, etc.
- cheat sheet
- callback function
- customizable shell by which programs are launched (sh -c, zsh -c, hyprctl dispatch exec, etc)
- minimalist, blazingly fast, keyboard-centric

# Installation

## AUR

### 1. Install with AUR helpers

```
paru -S otter-launcher
```

### 2. Create a config file

Otter-launcher reads from $HOME/.config/otter-launcher/config.toml. If that file is missing, it looks into /etc/otter-launcher/config.toml, which is included with AUR installation.

An example config file is at [config_example](https://github.com/kuokuo123/otter-launcher/tree/main/config_example). Copy it to one of the above locations. Also, check [more examples of module config](https://github.com/kuokuo123/otter-launcher/wiki) at the wiki page.

## Building from source

```
git clone https://github.com/kuokuo123/otter-launcher /tmp/otter-launcher
cd /tmp/otter-launcher
cargo build --release
sudo cp /tmp/otter-launcher/target/release/otter-launcher /usr/bin/
```


# Configuration

``` toml
[general]
default_module = "gg" # The module to run when no prefix is matched
empty_module = "app" # run with an empty prompt
exec_cmd = "sh -c" # The exec command of your shell, default to sh
# for example: "bach -c" for bash; "zsh -c" for zsh. This can also runs wm exec commands, like hyprctl dispatch exec
vi_mode = false # set true to use vi keybinds, false to use emacs keybinds; default to emacs
esc_to_abort = true # allow to quit with esc keypress; a useful option for vi users
cheatsheet_entry = "?" # when entered, otter-launcher will show a list of configured modules
cheatsheet_viewer = "less -R; clear" # the program that otter-launcher will pipe cheatsheet into
clear_screen_after_execution = false # useful when chafa image flash back after module execution
loop_mode = false # in loop mode, otter-launcher won't quit after running a module, useful when using scratchpad
external_editor = "" # if set, pressing ctrl+e (or pressing v in vi normal mode) will edit the input field in the specified program; default to no external editor
#callback = "" # if set, otter-launcher will run the command after a module is executed; for example, it can call swaymsg to adjust window size


# ASCII color codes are allowed with these options. However, \x1b should be replaced with \u001B (unicode escape) because the rust toml crate cannot read \x as an escaped character...
[interface]
# use three quotes to write longer commands
header = """ \u001B[34;1m  >\u001B[0m $USER@$(echo $HOSTNAME)            \u001B[31m\u001B[0m $(mpstat | awk 'FNR ==4 {print $4}')%  \u001B[33m󰍛\u001B[0m $(free -h | awk 'FNR == 2 {print $3}')\n    \u001B[34;1m>\u001B[0;1m """
# Run a shell command and make the stdout printed above the header
header_cmd = ""
header_cmd_trimmed_lines = 0 # Remove a number of lines from header_cmd output, in case of some programs printing excessive empty lines at the end of its output
header_concatenate = false # print header and header_cmd output at the same line, default to false
list_prefix = "      "
selection_prefix = "   \u001B[31;1m> "
place_holder = "type and search"
default_module_message = "      \u001B[33msearch\u001B[0m the internet" # if set, the text will be shown when the default module is in use
empty_module_message = "" # the text to show when empty module is in use
suggestion_mode = "list" # available options: list, hint
suggestion_lines = 4 # length of the suggestion list, set to 0 to disable suggestions and tab completion
indicator_with_arg_module = "\u001B[31m^\u001B[0m " # a sign showing whether the module should run with an argument
indicator_no_arg_module = "\u001B[31m$\u001B[0m "
prefix_padding = 3 # format prefixes to have a unified width; prefixes will be padded with spaces to have a least specified number of chars
# below color options affect all modules; per-module coloring is allowed by using ascii color codes at each module's configurations
prefix_color = "\u001B[33m"
description_color = "\u001B[38m"
place_holder_color = "\u001B[30m"
hint_color = "\u001B[30m" # the color of hint mode suggestions
# move the whole interface rightward or upward, easier for styling with chafa image
move_right = 0
move_up = 0


# Modules are defined as followed
[[modules]]
description = "search with google"
prefix = "gg"
# try wm's exec command if 'setsid -f' does not work, eg. 'hyprctl dispatch exec'
cmd = "setsid -f xdg-open 'https://www.google.com/search?q={}'"
with_argument = true # If "with_argument" is true, the {} in the cmd value will be replaced with user input. If the field is not explicitly set, will be taken as false.
url_encode = true # "url_encode" should be true if the module is set to call webpages, as this ensures special characters in url being readable to browsers. It'd better be false with shell scripts. If the field is not explicitly set, will be taken as false.

[[modules]]
description = "launch desktop apps"
prefix = "app"
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
description = "search in github"
prefix = "gh"
cmd = "setsid -f xdg-open https://github.com/search?q='{}'"
with_argument = true
url_encode = true

[[modules]]
description = "cambridge dict"
prefix = "dc"
cmd = "setsid -f xdg-open 'https://dictionary.cambridge.org/dictionary/english/{}'"
with_argument = true
url_encode = true

# fzf and fd are needed to run these functions
[[modules]]
description = "open files with fzf"
prefix = "fo"
cmd = "fd --type f | fzf | setsid -f xargs -r -I {} xdg-open '{}'"

[[modules]]
description = "open folders with fzf"
prefix = "yz"
cmd = "fd --type d | fzf | xargs -r -I {} yazi '{}'"
```

# Integration

Otter-launcher works well with tui programs. When launching programs, module.cmd can be scripted to perform functions like adjusting window size.

In the below example, otter-launcher changes window size before and after running pulsemixer by calling swaymsg:

```
[[modules]]
description = "pulsemixer for audio control"
prefix = "vol"
cmd = "swaymsg [app_id=otter-launcher] resize set width 600 px height 300 px; pulsemixer; swaymsg [app_id=otter-launcher] resize set width 600 px height 60 px"
```

Some recommendations of tui utilities that works really well:

- Desktop app launcher: [sway-launcher-desktop](https://github.com/Biont/sway-launcher-desktop)
- Audio control: [pulsemixer](https://github.com/GeorgeFilipkin/pulsemixer)
- Bluetooth control: [bluetui](https://github.com/pythops/bluetui) [bluetuith](https://github.com/darkhz/bluetuith)
- Wifi control: [nmtui](https://archlinux.org/packages/extra/x86_64/networkmanager/) [impala](https://github.com/pythops/impala)
- Spotify: [spotify_player](https://github.com/aome510/spotify-player)
- Mouse control: [wl-kbptr](https://github.com/moverest/wl-kbptr)

More on [Awesome TUIs](https://github.com/rothgar/awesome-tuis) or [Awesome Command Line(CLI/TUI) Programs](https://github.com/toolleeo/awesome-cli-apps-in-a-csv).

# Examples for Styling

## Example Config

![Example Config](./assets/default.png)

```
[interface]
header = """ \u001B[34;1m  >\u001B[0m $USER@$(echo $HOSTNAME)            \u001B[31m\u001B[0m $(mpstat | awk 'FNR ==4 {print $4}')%  \u001B[33m󰍛\u001B[0m $(free -h | awk 'FNR == 2 {print $3}')\n    \u001B[34;1m>\u001B[0;1m """
list_prefix = "      "
selection_prefix = "   \u001B[31;1m> "
place_holder = "type and search"
default_module_message = "      \u001B[33msearch\u001B[0m the internet"
suggestion_mode = "list"
suggestion_lines = 4
indicator_with_arg_module = "\u001B[31m^\u001B[0m "
indicator_no_arg_module = "\u001B[31m$\u001B[0m "
prefix_padding = 3
prefix_color = "\u001B[33m"
description_color = "\u001B[38m"
place_holder_color = "\u001B[30m"
hint_color = "\u001B[30m"
```

## Fastfetch & Krabby

![Fastfetch Config](./assets/fastfetch.png)

```
[interface]
header_cmd = "fastfetch --structure break:colors:break:os:wm:shell:kernel:term:uptime:datetime:break --key-type icon --logo-type data --logo \"$(krabby name pikachu --no-title)\""
header = "  \u001B[7;1m otter-launcher \u001B[0m "
header_cmd_trimmed_lines = 1
list_prefix = "    \u001B[36m-\u001B[0m "
selection_prefix = "    \u001B[31;1m> "
place_holder = ""
suggestion_mode = "list"
suggestion_lines = 5
indicator_with_arg_module = "\u001B[31m^\u001B[0m "
indicator_no_arg_module = "\u001B[31m$\u001B[0m "
prefix_padding = 3
prefix_color = "\u001B[33m"
description_color = "\u001B[38m"
place_holder_color = "\u001B[90m"
hint_color = "\u001B[90m"
```

## Image Protocol

![Foot Config](./assets/foot.png)

[Image Source: Artist Kat Corrigan & MWMO Stormwater Park](https://www.mwmo.org/learn/visit-us/exhibits/waterways-and-otterways/)

```
[interface]
header_cmd = "chafa --fit-width $HOME/.config/otter-launcher/images_other/waterways_and_otterways.jpg"
header_cmd_trimmed_lines = 1
header = """  \u001B[34;1m  󱎘 \u001B[0m $USER@$(echo $HOSTNAME)          \u001B[31m\u001B[0m $(mpstat | awk 'FNR ==4 {print $4}')%  \u001B[33m󰍛\u001B[0m $(free -h | awk 'FNR == 2 {print $3}')\n    \u001B[34;1m󱎘 \u001B[0;1m """
list_prefix = "       "
selection_prefix = "     \u001B[31;1m> "
place_holder = "type and search..."
default_module_message = """
       \u001B[35msearch\u001B[0m on the internet"""
suggestion_mode = "list"
suggestion_lines = 3
prefix_padding = 3
prefix_color = "\u001B[33m"
description_color = "\u001B[38m"
place_holder_color = "\u001B[90m"
hint_color = "\u001B[90m"
```

## Two Liner in Hint Mode

![Two_liner Config](./assets/two_liner.png)

```
[interface]
header = """  \u001B[34;1m  >\u001B[0m $USER@$(echo $HOSTNAME)            \u001B[31m\u001B[0m $(mpstat | awk 'FNR ==4 {print $4}')%  \u001B[33m󰍛\u001B[0m $(free -h | awk 'FNR == 2 {print $3}')\n     \u001B[34;1m>\u001B[0;1m """
indicator_with_arg_module = "^ "
indicator_no_arg_module = "$ "
place_holder = "type and search"
suggestion_mode = "hint"
place_holder_color = "\u001B[90m"
hint_color = "\u001B[90m"
```

## Image to the Left

![Chafa-text Config](./assets/soothing.png)

```
[interface]

# render image by chafa
header_cmd = "chafa -s x10 /home/kkoala/.config/otter-launcher/image.png"
header_cmd_trimmed_lines = 1

# move the layout
move_right = 19
move_up = 8

header = "  $USER@$(echo $HOSTNAME)     \u001B[31m\u001B[0m $(free -h | awk 'FNR == 2 {print $3}' | sed 's/i//')\n  "
list_prefix = "  "
selection_prefix = "\u001B[31;1m> "
place_holder = "type & search"
default_module_message = "  \u001B[33msearch\u001B[0m the internet"
suggestion_mode = "list"
suggestion_lines = 4
prefix_padding = 3
prefix_color = "\u001B[33m"
description_color = "\u001B[38m"
place_holder_color = "\u001B[90m"
hint_color = "\u001B[90m"
```

## Image to the Right

![Prinny Config](./assets/prinny.png)

This is a [prinny](https://github.com/kuokuo123/otter-launcher/tree/main/assets/prinny-raisehand.png), not really a penguin.

```
[interface]

# render image by chafa
header_cmd = """
printf "\u001B[30G"
chafa -s x10 /home/kkoala/.config/otter-launcher/images_rec.image
"""
header_cmd_trimmed_lines = 1

# move layout up
move_up = 9

# customized header & list prefix
header = """
    ┌ \u001B[1;34m  $USER@$(echo $HOSTNAME) \u001B[0m───┐
    │ \u001B[90m󱎘  \u001B[31m󱎘  \u001B[32m󱎘  \u001B[33m󱎘  \u001B[34m󱎘  \u001B[35m󱎘  \u001B[36m󱎘\u001B[0m │
    └ \u001B[36m \u001B[1;36m system\u001B[0m archlinux ┘
    ┌ \u001B[33m \u001B[1;36m window \u001B[0m     $XDG_CURRENT_DESKTOP ┐
    │ \u001B[31m \u001B[1;36m loads\u001B[0m      $(mpstat | awk 'FNR ==4 {print $4}')% │
    │ \u001B[32m \u001B[1;36m memory\u001B[0m     $(free -h | awk 'FNR == 2 {print $3}') │
    │ \u001B[90m\u001B[0m  """
list_prefix = "    └ \u001B[34m󱓞  "
default_module_message = "    └ \u001B[34m󱓞  \u001B[33msearch\u001B[0m the internet"

place_holder = "type & search"
suggestion_mode = "list"
suggestion_lines = 1
prefix_color = "\u001B[33m"
description_color = "\u001B[38m"
place_holder_color = "\u001B[90m"
hint_color = "\u001B[90m"
```
