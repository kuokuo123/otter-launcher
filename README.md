
# otter-launcher

![cover_pic](./assets/default.png)

A very hackable app launcher, designed for keyboard-centric wm users. It is blazingly fast, supports vi and emacs keybinds, and can be decorated with ansi color codes, sixel or kitty image protocols. Plus, through bash scripting, system info widgets can be added to the infinity.

The core concept is making these behaviours possible:

- type "gg margaret thatcher" to google the lady in a web browser
- "sh htop" to run htop in a terminal
- "dc linux" to search the word linux with an online dictionary
- "app" to launch application menu
- etc.

Some helper scripts can be found in the [contrib](https://github.com/kuokuo123/otter-launcher/tree/main/contrib) folder. Use your wm's window rules to control its window size.

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
- supporting ansi codes, chafa, sixel or kitty image protocol, etc.
- overlay layer to show chafa image
- cheat sheet
- callback function
- customizable shell by which programs are launched (sh -c, zsh -c, hyprctl dispatch exec, etc)
- minimalist, blazingly fast, keyboard-centric

# Installation

### AUR

Install with AUR helpers

```
paru -S otter-launcher
```

### Building from source

1. Compile from source code

```
git clone https://github.com/kuokuo123/otter-launcher /tmp/otter-launcher
cd /tmp/otter-launcher
cargo build --release
sudo cp /tmp/otter-launcher/target/release/otter-launcher /usr/bin/
```

2. Create a config file mannually

Put a config at $HOME/.config/otter-launcher/config.toml. The [default config](https://github.com/kuokuo123/otter-launcher/tree/main/config_example/config.toml) looks for /etc/otter-launcher/[pikachu.example](https://github.com/kuokuo123/otter-launcher/tree/main/config_example/pikachu.example) to show a chafa image. You can modify the config file to remove this line from overlay_cmd.

# Configuration

Otter-launcher reads from $HOME/.config/otter-launcher/config.toml. If missing, it looks into /etc/otter-launcher/config.toml, which is included in AUR installation.

The confing file encompasses four parts:

- [general] includes generic options
- [interface] includes options related to user interface
- [overlay] includes options releated to image integration
- [[modules]] can be configured through bash scripting in an unlimited number

All the available options are listed below. Also, check [more examples of module config](https://github.com/kuokuo123/otter-launcher/wiki) at the wiki page.

Since v0.6.1, the default config comes with riced fzf modules, and a chafa otter to demonstrate how image integration works. v0.6.0 comes with a pikachu.

![Example Config](./assets/default.png)

``` toml
[general]
default_module = "gg" # module to run when no prefix is matched
empty_module = "app" # run with an empty prompt
exec_cmd = "sh -c" # exec command of your shell
# for example: "bach -c" for bash; "zsh -c" for zsh; also accept wm commands like "hyprctl dispatch exec"
vi_mode = false # set true to use vi keybinds, false emacs keybinds
esc_to_abort = true # useful for vi users
cheatsheet_entry = "?" # when prompted, will show a list of configured modules
cheatsheet_viewer = "less -R; clear" # command to show cheatsheet; through piping stdout
clear_screen_after_execution = false
loop_mode = false # don't quit after executing a module, useful with scratchpads
external_editor = "vi" # if set, press ctrl+e (or v in vi normal mode) to edit prompt in specified program
delay_startup = 0 # sometimes the otter runs too fast even before the terminal window is ready; this slows it down by milliseconds; useful when chafa image is skewed
#callback = "" # if set, will run after module execution; for example, calling swaymsg to adjust window size


# ANSI color codes are allowed. However, \x1b should be replaced with \u001B, because the rust toml crate cannot read \x as an escaped character
[interface]
# use three quotes to write longer codes
header = """
  \u001B[34;1m$USER@$(printf $HOSTNAME)\u001B[0m     \u001B[31m\u001B[0m $(mpstat | awk 'FNR ==4 {print $4}')
  """
header_cmd = "" # run a command and print stdout above the header
header_cmd_trimmed_lines = 0 # remove trailing lines from header_cmd output, in case of some programs appending excessive empty lines
header_concatenate = false # print header and header_cmd output to the same line, default to false
place_holder = "otterly awesome" # at the input field
suggestion_mode = "list" # available options: list, hint
separator = "                      \u001B[90mmodules ────────────────" # add a line between intput field and suggestion list; only effective in list mode
footer = "" # add a line after suggestion list
suggestion_lines = 4 # 0 to disable suggestions and tab completion
list_prefix = "  "
selection_prefix = "\u001B[31;1m▌ "
prefix_padding = 3 # format prefixes to have a uniformed width
default_module_message = "  \u001B[33msearch\u001B[0m the internet" # shown when the default module is in use
empty_module_message = "" # shown when the empty module is in use
customized_list_order = false # false to list modules alphabetically; true to list as per the configured order in the below [[modules]] section
indicator_with_arg_module = "^ " # the sign showing whether a module should run with an argument
indicator_no_arg_module = "$ "
# below color options affect all modules; per-module coloring can be configured using ansi codes individually
prefix_color = "\u001B[33m"
description_color = "\u001B[39m"
place_holder_color = "\u001B[30m"
hint_color = "\u001B[30m" # suggestion color in hint mode
# move the interface rightward or downward
move_interface_right = 20
move_interface_down = 1


# overlay is a floating layer that can be printed with stdout and moved around; useful for integrating chafa images
[overlay]
# run a command and print stdout on the overlay layer
overlay_cmd = """
cat /etc/otter-launcher/pikachu.example \
|| echo -e "The file pickachu.example is not found. Pikachu can be at the below blank area. Fix this by modifying the overlay_cmd option in your config file.\n\n"
"""
overlay_trimmed_lines = 0 # remove trailing lines from overlay_cmd output
overlay_height = 0 # set overlay size; 0 to be auto; 1 is one line, 2 two lines, etc; kitty & sixel image size can be determined automatically; others should be set mannually
move_overlay_right = 0 # move the overlay layer around for theming
move_overlay_down = 0


# modules are defined as followed
[[modules]]
description = "google search"
prefix = "gg"
cmd = "xdg-open https://www.google.com/search?q='{}'" # try wm's exec command for unbinding if 'setsid -f' does not work as expected, eg. 'hyprctl dispatch exec'
with_argument = true # if true, {} in cmd will be replaced with user input. if not explicitly set, taken as false.
url_encode = true # should be true when calling webpages; this ensures special characters in url being readable to browsers; taken as false if not explicitly set
unbind_proc = true # run cmd in a forked shell as opposed to as a child process; useful for launching gui programs; taken as false if not explicitly set

# fzf is needed to run below functions
[[modules]]
description = "desktop programs"
prefix = "app"
cmd = """
desktop_file() {
find /usr/share/applications -name "*.desktop" 2>/dev/null
find /usr/local/share/applications -name "*.desktop" 2>/dev/null
find "$HOME/.local/share/applications" -name "*.desktop" 2>/dev/null
find /var/lib/flatpak/exports/share/applications -name "*.desktop" 2>/dev/null
find "$HOME/.local/share/flatpak/exports/share/applications" -name "*.desktop" 2>/dev/null
}
selected="$(desktop_file | sed 's/.desktop$//g' | sort | fzf --reverse --padding 1,3 --info-command 'echo -e " desktop apps ($FZF_POS/$FZF_TOTAL_COUNT)"' --cycle --pointer " ▌" --color "bg+:-1,pointer:1,info:8,separator:8,scrollbar:0" --prompt '  ' -m -d / --with-nth -1 )"
[ -z "$selected" ] && exit
echo "$selected" | while read -r line ; do setsid -f gtk-launch "$(basename $line)"; done
"""

[[modules]]
description = "power menu (fzf)"
prefix = "po"
cmd = """
function power {
if [[ -n $1 ]]; then
case $1 in
"logout") session=`loginctl session-status | head -n 1 | awk '{print $1}'`; loginctl terminate-session $session ;;
"suspend") systemctl suspend ;;
"hibernate") systemctl hibernate ;;
"reboot") systemctl reboot ;;
"shutdown") systemctl poweroff ;;
esac fi }
power $(echo -e 'reboot\nshutdown\nlogout\nsuspend\nhibernate' | fzf --reverse --padding 1,2 --info-command 'printf " power menu ($FZF_POS/$FZF_TOTAL_COUNT)"' --cycle --pointer " ▌" --color "bg+:-1,pointer:1,info:8,separator:8,scrollbar:0" --prompt '  ' | tail -1)
"""

[[modules]]
description = "run commands"
prefix = "sh"
cmd = """
$(printf $TERM | sed 's/xterm-//g') -e sh -c "{}"
"""
with_argument = true
unbind_proc = true

[[modules]]
description = "search archwiki"
prefix = "aw"
cmd = "xdg-open https://wiki.archlinux.org/index.php?search='{}'"
with_argument = true
url_encode = true
unbind_proc = true

[[modules]]
description = "search packages"
prefix = "pac"
cmd = "xdg-open https://archlinux.org/packages/?q='{}'"
with_argument = true
url_encode = true
unbind_proc = true

[[modules]]
description = "search the AUR"
prefix = "aur"
cmd = "xdg-open https://aur.archlinux.org/packages?K='{}'"
with_argument = true
url_encode = true
unbind_proc = true

[[modules]]
description = "cambridge dict"
prefix = "dc"
cmd = "xdg-open 'https://dictionary.cambridge.org/dictionary/english/{}'"
with_argument = true
url_encode = true
unbind_proc = true

[[modules]]
description = "open files (fzf)"
prefix = "fo"
cmd = """
find $HOME -type f -not -path '*/.cache/*' 2>/dev/null | fzf --reverse --padding 1,3 --info-command 'printf " files ($FZF_POS/$FZF_TOTAL_COUNT)"' --cycle --pointer ' ▌' --color 'bg+:-1,pointer:1,info:8,separator:8,scrollbar:0' --prompt '  ' | setsid -f xargs -r -I [] xdg-open '[]'
"""

[[modules]]
description = "open dirs (yazi)"
prefix = "yz"
cmd = """
find $HOME -type d -not -path '*/.cache/*' 2>/dev/null | fzf --reverse --padding 1,3 --info-command 'printf " directories ($FZF_POS/$FZF_TOTAL_COUNT)"' --cycle --pointer ' ▌' --color 'bg+:-1,pointer:1,info:8,separator:8,scrollbar:0' --prompt '  ' | xargs -r -I [] setsid -f "$(echo $TERM | sed 's/xterm-//g')" -e yazi '[]'
"""
```

# Integration

Otter-launcher works well with tui programs, and module.cmd can be scripted to adjust window sizes. In the below example, otter-launcher changes window size before and after running pulsemixer by calling swaymsg:

``` toml
[[modules]]
description = "pulsemixer for audio control"
prefix = "vol"
cmd = "swaymsg [app_id=otter-launcher] resize set width 600 px height 300 px; pulsemixer; swaymsg [app_id=otter-launcher] resize set width 600 px height 60 px"
```

Some recommendations of tui utilities that works really well:

- Desktop app launcher: [sway-launcher-desktop](https://github.com/Biont/sway-launcher-desktop) [fsel](https://github.com/Mjoyufull/fsel)
- Audio control: [pulsemixer](https://github.com/GeorgeFilipkin/pulsemixer)
- Bluetooth control: [bluetui](https://github.com/pythops/bluetui) [bluetuith](https://github.com/darkhz/bluetuith)
- Wifi control: [nmtui](https://archlinux.org/packages/extra/x86_64/networkmanager/) [impala](https://github.com/pythops/impala)
- Spotify: [spotify_player](https://github.com/aome510/spotify-player)
- Mouse control: [wl-kbptr](https://github.com/moverest/wl-kbptr)
- More on [Awesome TUIs](https://github.com/rothgar/awesome-tuis) or [Awesome Command Line(CLI/TUI) Programs](https://github.com/toolleeo/awesome-cli-apps-in-a-csv).

Also, it's recommended to setup a dedicated desktop app launcher as a module, like [sway-launcher-desktop](https://github.com/Biont/sway-launcher-desktop) (more mature) or [fsel](https://github.com/Mjoyufull/fsel) (developing). The one that comes in default config is just a simple script finding into regular POSIX dirs and flatpak. If your apps are from different sources, it won't show.

# Examples for Styling

## Two Liner in Hint Mode

![Two_liner Config](./assets/two_liner.png)

```toml
[interface]
header = """  \u001B[34;1m  >\u001B[0m $USER@$(echo $HOSTNAME)              \u001B[31m\u001B[0m $(cat /proc/loadavg | cut -d ' ' -f 1)  \u001B[33m󰍛\u001B[0m $(free -h | awk 'FNR == 2 {print $3}' | sed 's/i//')\n     \u001B[34;1m>\u001B[0;1m """
indicator_with_arg_module = "^ "
indicator_no_arg_module = "$ "
place_holder = "type and search"
suggestion_mode = "hint"
place_holder_color = "\u001B[90m"
hint_color = "\u001B[90m"
```

## Fastfetch & Krabby

![Fastfetch Config](./assets/fastfetch.png)

``` toml
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
description_color = "\u001B[39m"
place_holder_color = "\u001B[90m"
hint_color = "\u001B[90m"
```

## Image Protocol

This config uses chafa in header_cmd to render the image.

![Foot Config](./assets/foot.png)

[Image Source: Artist Kat Corrigan & MWMO Stormwater Park](https://www.mwmo.org/learn/visit-us/exhibits/waterways-and-otterways/)

``` toml
[interface]
header_cmd = "chafa --fit-width $HOME/.config/otter-launcher/images_other/waterways_and_otterways.jpg"
header_cmd_trimmed_lines = 1
header = """  \u001B[34;1m  󱎘 \u001B[0m $USER@$(echo $HOSTNAME)          \u001B[31m\u001B[0m $(cat /proc/loadavg | cut -d ' ' -f 1)  \u001B[33m󰍛\u001B[0m $(free -h | awk 'FNR == 2 {print $3}')\n    \u001B[34;1m󱎘 \u001B[0;1m """
list_prefix = "       "
selection_prefix = "     \u001B[31;1m> "
place_holder = "type and search..."
default_module_message = """
       \u001B[35msearch\u001B[0m on the internet"""
suggestion_mode = "list"
suggestion_lines = 3
prefix_padding = 3
prefix_color = "\u001B[33m"
description_color = "\u001B[39m"
place_holder_color = "\u001B[90m"
hint_color = "\u001B[90m"
```

## Image to the Left

This config renders chafa image by overlay_cmd at the left, and move the whole inteface to the right.

![Chafa-text Config](./assets/soothing.png)

```toml
[overlay]
# render image in overlay layer using chafa
overlay_cmd = "chafa -s x10 $HOME/.config/otter-launcher/image.png"
overlay_trimmed_lines = 1

[interface]
header = "  $USER@$(echo $HOSTNAME)     \u001B[31m\u001B[0m $(free -h | awk 'FNR == 2 {print $3}' | sed 's/i//')\n  "
list_prefix = "  "
selection_prefix = "\u001B[31;1m> "
place_holder = "type & search"
default_module_message = "  \u001B[33msearch\u001B[0m the internet"
suggestion_mode = "list"
suggestion_lines = 4
prefix_padding = 3
prefix_color = "\u001B[33m"
description_color = "\u001B[39m"
place_holder_color = "\u001B[90m"
hint_color = "\u001B[90m"

# move the interface
move_interface_right = 20
move_interface_down = 2
```

## Image to the Right

This config also renders a [prinny](https://github.com/kuokuo123/otter-launcher/tree/main/assets/prinny-raisehand.png) using overlay_cmd, and then move the overlay to the right.

![Prinny Config](./assets/prinny.png)

```toml
[overlay]
# render image in overlay layer using chafa
overlay_cmd = "chafa -s x10 $HOME/.config/otter-launcher/image.png"
overlay_trimmed_lines = 1

#move overlay rightwards
move_overlay_right = 32

[interface]
# move the interface
move_interface_down = 1

# customized header & list prefix
header = """
    ┌ \u001B[1;34m  $USER@$(echo $HOSTNAME) \u001B[0m───┐
    │ \u001B[90m󱎘  \u001B[31m󱎘  \u001B[32m󱎘  \u001B[33m󱎘  \u001B[34m󱎘  \u001B[35m󱎘  \u001B[36m󱎘\u001B[0m │
    └ \u001B[36m \u001B[1;36m system\u001B[0m archlinux ┘
    ┌ \u001B[33m \u001B[1;36m window \u001B[0m     $XDG_CURRENT_DESKTOP ┐
    │ \u001B[31m \u001B[1;36m loads\u001B[0m       $(cat /proc/loadavg | cut -d ' ' -f 1) │
    │ \u001B[32m \u001B[1;36m memory\u001B[0m     $(free -h | awk 'FNR == 2 {print $3}') │
    │ \u001B[90m\u001B[0m  """
list_prefix = "    └ \u001B[34m󱓞  "
selection_prefix = "    └ \u001B[31m󱓞  "
default_module_message = "    └ \u001B[34m󱓞  \u001B[33msearch\u001B[0m the internet"

place_holder = "type & search"
suggestion_mode = "list"
suggestion_lines = 1
prefix_color = "\u001B[33m"
description_color = "\u001B[39m"
place_holder_color = "\u001B[90m"
hint_color = "\u001B[90m"
```
