#!/bin/sh
# A Windows 11 virtual machine in Docker (QEMU with KVM, by the dockur
# project), for trying Heyo on Windows.
#
#   scripts/windows-test/run.sh start     create or start the machine
#   scripts/windows-test/run.sh logs      follow its output
#   scripts/windows-test/run.sh stop      shut it down (its disk is kept)
#
# The first start downloads Windows and installs it unattended, which takes
# a good while. Watch and use it in a browser at http://127.0.0.1:8006, or
# by RDP on 127.0.0.1:13389 (user "Docker", password "admin"); both are
# reachable from this computer only. The machine's disk lives in
# $HEYO_WINDOWS_DIR (default ~/vms/heyo-windows); the repository shows up in
# it as the network folder \\host.lan\Data (the "Shared" folder on its
# desktop), writable: build.sh puts a Windows build of Heyo there to run.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
NAME=heyo-windows
STORAGE=${HEYO_WINDOWS_DIR:-$HOME/vms/heyo-windows}
case "$(uname -m)" in
  aarch64|arm64) IMAGE=dockurr/windows-arm ;;
  *) IMAGE=dockurr/windows ;;
esac

case "${1:-}" in
  start)
    if [ -n "$(docker ps -aq -f name="^$NAME\$")" ]; then
      docker start "$NAME" >/dev/null
    else
      mkdir -p "$STORAGE"
      docker run -d --name "$NAME" \
        --device /dev/kvm --device /dev/net/tun --cap-add NET_ADMIN \
        -e VERSION=11 -e RAM_SIZE="${RAM_SIZE:-8G}" -e CPU_CORES="${CPU_CORES:-4}" -e DISK_SIZE="${DISK_SIZE:-64G}" \
        -p 127.0.0.1:8006:8006 -p 127.0.0.1:13389:3389/tcp -p 127.0.0.1:13389:3389/udp \
        -v "$STORAGE:/storage" -v "$REPO:/data" \
        --stop-timeout 120 "$IMAGE" >/dev/null
    fi
    echo "started $NAME: http://127.0.0.1:8006"
    ;;
  logs) docker logs -f --tail 40 "$NAME" ;;
  stop) docker stop -t 120 "$NAME" >/dev/null 2>&1 && echo stopped || echo "not running" ;;
  *) echo "usage: $0 start|logs|stop" >&2; exit 2 ;;
esac
