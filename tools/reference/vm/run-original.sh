#!/bin/bash
# Runs inside the emulated PC (copied there by setup.sh). Starts the display and VNC if they are
# not running, then the original Banished from Steam's library, with Steam already signed in.
#   bash run-original.sh [SECONDS]   start the game, wait SECONDS (default 40), print its state
#   bash run-original.sh stop        stop only the game
# The game's own launcher opens first; press Play in it (xdotool or VNC).
export DISPLAY=:1 WINEPREFIX=$HOME/.wine-banished LP_NATIVE_VECTOR_WIDTH=128
# With the app id set, steam_api does not hand the start to steam.exe (which exits with 53).
export SteamAppId=242920 SteamGameId=242920
export WINEDEBUG=err+all,warn+seh,+timestamp
# UNBANISHED_VM_AUDIO=off: XAudio2 cannot start, and Audio::Device::Startup (1800e0cd0) carries on
# without a device. Then the audio makes no draws from the game's Random, which in the VM come far
# more often per iteration than on a PC, and Wine's WMA decoder thread cannot hang the game.
[[ ${UNBANISHED_VM_AUDIO:-on} == off ]] && export WINEDLLOVERRIDES="xaudio2_7=d"
steam="$WINEPREFIX/drive_c/Program Files (x86)/Steam"
# Patterns that cannot match this script's own command line.
game='steamapps.common.Banished.Applicatio[n]'

for p in $(pgrep -f "$game"); do kill "$p"; done
[[ ${1:-} == stop ]] && exit 0

pgrep -x Xvfb >/dev/null || { Xvfb :1 -screen 0 1280x720x24 >/tmp/xvfb.log 2>&1 & sleep 2; }
pgrep -x openbox >/dev/null || { openbox >/tmp/openbox.log 2>&1 & }
if ! pgrep -x x11vnc >/dev/null && [[ -f ~/.vnc-pass ]]; then
  x11vnc -display :1 -localhost -rfbauth ~/.vnc-pass -forever -shared -rfbport 5900 >/tmp/x11vnc.log 2>&1 &
fi
if ! pgrep -f '[S]team.exe' >/dev/null; then
  (cd "$steam" && wine Steam.exe -silent -no-cef-sandbox -cef-disable-gpu -cef-disable-gpu-compositing >/tmp/steam.log 2>&1 &)
  echo "started Steam; give it a minute to sign in"
  sleep 60
fi
# Steam's overlay crashes in Direct3D under Wine here; keep it switched off.
for f in GameOverlayRenderer64.dll GameOverlayRenderer.dll; do
  [[ -f "$steam/$f" ]] && mv "$steam/$f" "$steam/$f.off"
done

sleep 3
rm -f "$steam/steamapps/common/Banished/crash.dmp"
cd "$steam/steamapps/common/Banished"
wine Application-steam-x64.exe >/tmp/banished.log 2>&1 &
echo $! >/tmp/banished.pid
sleep "${1:-40}"
kill -0 "$(cat /tmp/banished.pid)" 2>/dev/null && echo running || echo exited
grep -E 'seh|err:' /tmp/banished.log | grep -v ntlm | tail -5
import -window root /tmp/banished.png && echo "screen: /tmp/banished.png"
