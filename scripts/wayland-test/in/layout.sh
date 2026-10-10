#!/bin/sh
# Inside the test session: how the Shell has the monitors laid out. For each:
# its rectangle and scale in the Shell's units; then how many pixels one unit
# of the stage is at most (1 when monitors are laid out in pixels).
exec "$(dirname "$0")/shell.py" '({
  monitors: Main.layoutManager.monitors.map((m, i) => ({x: m.x, y: m.y, width: m.width, height: m.height, scale: global.display.get_monitor_scale(i)})),
  stage: global.stage.get_resource_scale(),
  pointer: global.get_pointer().slice(0, 2),
})'
