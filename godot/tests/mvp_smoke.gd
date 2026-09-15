# MVP acceptance smoke (backlog 6.2) — the full player loop, headless.
#
# Not a re-run of smoke.gd: this is the "as the player" end-to-end
# scenario of the MVP cut: fresh world -> read the shipped demo from
# scripts/ -> Run (the runner's attach path) -> the world drives ->
# the rover patrols with console reports -> Stop freezes -> re-Run
# drives again. Console reports are captured from the log_line signal
# (the bridge-side drain of the core LogBuffer).
#
# Run from godot/:
#   godot --headless --script res://tests/mvp_smoke.gd
# Exit code 0 and "MVP SMOKE OK" = the MVP loop works end to end.
extends SceneTree

var _failures := 0


func _initialize() -> void:
	# Deferred: the checks need a live tree (physics frames drive ticks).
	_run.call_deferred()


func _run() -> void:
	# 1. Clean state: a fresh world with one rover on the start pad.
	var sim := SimNode.new()
	root.add_child(sim)
	var id := sim.get_rover_ids()[0]
	var start := sim.get_rover_position(id)
	_check(sim.get_tick() == 0, "fresh world starts at tick 0")

	# 2. The player's script folder: read the shipped demo.
	var files = load("res://scripts/script_files.gd").new()
	var source: String = files.read("patrol.lua")
	_check(not source.is_empty(), "scripts/patrol.lua read from the player folder")

	# 3. Run (the runner's path): attach through the simulation host.
	var sid := sim.attach_script(id, source)
	_check(sid > 0, "Run: patrol.lua compiles and attaches")

	var captured: Array = []
	sim.log_line.connect(
		func(_tick: int, _rover: int, text: String): captured.append(text)
	)

	# 4. The world drives; the rover patrols the square route.
	var frame := await _poll(
		func() -> bool: return sim.get_rover_position(id).distance_to(start) > 1.0, 240
	)
	_check(frame >= 0, "rover moved along the route (frame %d)" % frame)
	frame = await _poll(
		func() -> bool: return captured.size() >= 1 and str(captured[0]).contains("патруль"),
		120
	)
	_check(frame >= 0, "console shows patrol reports (frame %d)" % frame)
	_check(
		str(sim.get_script_state(sid).get("state")) == "running",
		"script state is running"
	)
	frame = await _poll(
		func() -> bool: return sim.get_rover_position(id).distance_to(Vector2.ZERO) < 1.0, 240
	)
	_check(frame >= 0, "first route corner (0,0) reached (frame %d)" % frame)

	# 5. Stop: script parked, command queue cleared - the rover freezes.
	sim.stop_script(sid)
	sim.clear_rover_commands(id)
	var frozen := sim.get_rover_position(id)
	frame = await _poll(
		func() -> bool: return sim.get_rover_position(id).distance_to(frozen) > 0.5, 90
	)
	_check(frame == -1, "Stop: rover frozen in place")

	# 6. Re-Run: a fresh script id, and the rover drives again (after the
	# demo's corner pause the route continues to the next corner).
	var sid2 := sim.attach_script(id, source)
	_check(sid2 > 0 and sid2 != sid, "re-Run attaches a fresh script")
	frame = await _poll(
		func() -> bool: return sim.get_rover_position(id).distance_to(frozen) > 1.0, 240
	)
	_check(frame >= 0, "rover moves again after re-Run (frame %d)" % frame)

	sim.queue_free()
	_finish()


## Poll until the predicate holds, at most `budget` process frames.
func _poll(predicate: Callable, budget: int) -> int:
	for i in range(budget):
		if predicate.call():
			return i
		await process_frame
	return -1


func _check(condition: bool, message: String) -> void:
	if condition:
		print("ok   - ", message)
	else:
		_failures += 1
		push_error("FAIL - " + message)


func _finish() -> void:
	if _failures == 0:
		print("MVP SMOKE OK")
		quit(0)
	else:
		print("MVP SMOKE FAILED: %d check(s)" % _failures)
		quit(1)
