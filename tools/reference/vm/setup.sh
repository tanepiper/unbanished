#!/usr/bin/env bash
# Write and start this project's emulated PC (derived/vm, git-ignored) with GameFrame's emupc.py,
# then copy run-original.sh into it. Run it once; afterwards derived/vm/start.sh starts the VM.
# See tools/reference/README.md ("The original in an emulated PC").
set -euo pipefail
here="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$here"
vm=derived/vm
image="${UNBANISHED_VM_IMAGE:?set UNBANISHED_VM_IMAGE to the Debian cloud image (tools/gf.sh tools/live/emupc.py image)}"
if [[ ! -f $vm/ssh_config ]]; then
  mkdir -p "$vm"
  [[ -f $vm/id_ed25519 ]] || ssh-keygen -q -t ed25519 -N '' -C gf-vm-banished -f "$vm/id_ed25519"
  tools/gf.sh tools/live/emupc.py init "$vm" --image "$image" --key "$vm/id_ed25519.pub" \
    --memory 8G --cpus 6 --ssh-port 2242 --disk 30G
  # Banished is a 64-bit program: add 64-bit Mesa beside emupc's 32-bit libraries, the WMA
  # decoder xaudio2 needs for the game's xWMA sounds, and a VNC server to see the screen.
  sed -i '' 's/libgl1-mesa-dri:i386$/libgl1-mesa-dri:i386 libgl1 libglx-mesa0 libgl1-mesa-dri gstreamer1.0-libav gstreamer1.0-plugins-good x11vnc/' \
    "$vm/cloud-init/user-data"
fi
"$vm/start.sh"
ssh_vm=(ssh -F "$vm/ssh_config" gf-vm-banished-wasm)
echo "waiting for the first boot (about 7 minutes)"
until "${ssh_vm[@]}" -o ConnectTimeout=10 test -f /var/lib/gameframe-ready 2>/dev/null; do sleep 20; done
# A silent sound device: the VM has no sound card, and tests are never heard.
"${ssh_vm[@]}" 'printf "pcm.!default { type null }\nctl.!default { type hw card 0 }\n" > ~/.asoundrc'
scp -q -F "$vm/ssh_config" tools/reference/vm/run-original.sh gf-vm-banished-wasm:run-original.sh
echo "ready. Next: install Steam in the VM and sign in (tools/reference/README.md)"
