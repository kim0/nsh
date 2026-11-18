#!/usr/bin/env bash
# Fork-only demo: in examples/counter on a real GTK window under Xvfb, hold
# Return and then Space on a focused button with X11 auto-repeat on. Take a
# screenshot after each step and count the KeyPress repeats X delivered.
set -euo pipefail
mkdir -p shots
openbox >/dev/null 2>&1 &
sleep 1
xset r on
xset r rate 500 30
xset q | grep -A1 -i 'auto repeat' || true

./target/debug/nsh run examples/counter --target linux >nsh-run.log 2>&1 &
app=$!
trap 'kill "$app" 2>/dev/null || true; tail -n 40 nsh-run.log || true' EXIT

window=
for _ in $(seq 1 900); do
  window=$(xdotool search --onlyvisible --name '.+' 2>/dev/null | head -n 1 || true)
  [ -n "$window" ] && break
  kill -0 "$app" || { echo 'nsh run exited before a window showed'; exit 1; }
  sleep 1
done
[ -n "$window" ] || { echo 'no window within 900 s'; exit 1; }
echo "window $window: $(xdotool getwindowname "$window")"
xdotool windowactivate --sync "$window" || xdotool windowfocus --sync "$window"
sleep 3

xinput test-xi2 --root >xi2.log 2>&1 &
recorder=$!
shot() { import -window root "shots/$1.png"; echo "shot $1"; }
repeats() { grep -c 'flags: repeat' xi2.log || true; }

shot 0-start
xdotool key --delay 300 Tab Tab
sleep 1
shot 1-focused

xdotool keydown Return
sleep 1.5
xdotool keyup Return
sleep 1
echo "repeats after Return: $(repeats)"
shot 2-held-return

xdotool keydown space
sleep 1.5
xdotool keyup space
sleep 1
echo "repeats after Return and Space: $(repeats)"
shot 3-held-space

xdotool key Return
sleep 1
shot 4-fresh-return
kill "$recorder" 2>/dev/null || true
