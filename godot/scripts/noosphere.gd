# Noosphere pulse prototype (backlog 6.5.6) — two facade indices:
# Energy and Ecology, 0..100.
#
# THIS IS A MOOD PROTOTYPE, NOT SIMULATION (documented disclaimer): the
# formulas are placeholder balance until phase 7 moves the real indices
# (ecology/trust/harmony) into neogen-core; these HUD bars will then
# mirror the core values instead of self-computing. The core, its ticks
# and determinism are untouched - the model lives entirely on the Godot
# side and feeds on bridge-readable signals only.
#
# Event sources (documented limitation): movement is the polled
# get_rover_speed (reliable); scans are detected as increases of the
# rover's scan-buffer length - the buffer is capped (16), so without a
# drain the counter saturates and scan events stop registering; the
# phase-7 core events replace this heuristic.
#
# Placeholder balance (all constants - tune freely in 6.5.7):
#   - moving:  energy -0.9/s, ecology -0.06/s
#   - a scan:  energy +2.0 (data harvest), ecology -1.2 (biopshere
#              disturbance)
#   - idle:    energy +0.5/s (solar recharge by the settlement panels),
#              ecology +0.25/s (the world calms down)
extends RefCounted

const MOVE_ENERGY_DRAIN := 0.9    # per second while driving
const MOVE_ECO_DRAIN := 0.06      # per second while driving
const SCAN_ENERGY_BOOST := 2.0    # per completed scan
const SCAN_ECO_COST := 1.2        # per completed scan
const IDLE_ENERGY_REGEN := 0.5    # per second parked
const IDLE_ECO_REGEN := 0.25      # per second parked

var energy := 80.0
var ecology := 90.0


## Advance the model. `moving` - the rover is driving; `scans` - how many
## scans completed since the previous update (event count, not a level).
func update(dt: float, moving: bool, scans: int) -> void:
	if moving:
		energy -= MOVE_ENERGY_DRAIN * dt
		ecology -= MOVE_ECO_DRAIN * dt
	else:
		energy += IDLE_ENERGY_REGEN * dt
		ecology += IDLE_ECO_REGEN * dt
	energy += scans * SCAN_ENERGY_BOOST
	ecology -= scans * SCAN_ECO_COST
	energy = clampf(energy, 0.0, 100.0)
	ecology = clampf(ecology, 0.0, 100.0)
