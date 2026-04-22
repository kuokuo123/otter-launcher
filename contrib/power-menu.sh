#!/bin/sh

# Rewrite of the script display on the README but POSIX
# Depend on logind

power() {
    case "$1" in
        logout)   session=$(loginctl session-status | head -n 1 | awk '{print $1}'); loginctl terminate-session "$session" ;;
        suspend)  loginctl suspend ;;
        hibernate) loginctl hibernate ;;
        reboot)   loginctl reboot ;;
        shutdown) loginctl poweroff ;;
    esac
}

power "$(printf 'reboot\nshutdown\nlogout\nsuspend\nhibernate\n' | fzf --reverse --no-scrollbar --padding 1,3 --prompt 'Power Menu: ' | tail -1)"
