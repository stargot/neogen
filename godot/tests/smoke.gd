# Neogen bridge smoke test (backlog 3.2).
# Run (from godot/, after building the extension and one editor import):
#   godot --headless --script res://tests/smoke.gd
# Exit code 0 = all checks passed.

extends SceneTree

var _failures: int = 0


func _initialize() -> void:
	var sim := SimNode.new()
	root.add_child(sim)
	check(sim.get_tick() == 0, "fresh sim starts at tick 0")

	var ids := sim.get_rover_ids()
	check(ids.size() == 1, "one rover in a fresh world, got %d" % ids.size())
	var id := ids[0]

	# Determinism: two sims with the same seed start identically.
	var sim2 := SimNode.new()
	root.add_child(sim2)
	var ids2 := sim2.get_rover_ids()
	check(ids2.size() == 1, "second sim has one rover")
	check(
		sim.get_rover_position(id) == sim2.get_rover_position(ids2[0]),
		"same seed -> same seeded start position"
	)

	# Ticking: manual stepping is exact; without commands the rover parks.
	var start := sim.get_rover_position(id)
	sim.step_ticks(10)
	check(sim.get_tick() == 10, "tick counter == 10, got %d" % sim.get_tick())
	check(sim.get_rover_position(id) == start, "no commands -> rover parks")

	# Known movement + coordinate conversion (core (3, 4) -> Godot (3, -4)).
	check(sim.debug_move_rover(id, 3.0, 4.0), "debug move queued")
	sim.step_ticks(100)
	var target := Vector2(3.0, -4.0)
	var position := sim.get_rover_position(id)
	check(
		(position - target).length() < 0.000001,
		"rover reached core (3,4) as screen (3,-4), got %s" % position
	)

	# Stepping in two chunks equals stepping in one (determinism of the loop).
	sim2.debug_move_rover(ids2[0], 3.0, 4.0)
	sim2.step_ticks(37)
	sim2.step_ticks(63)
	check(
		sim.get_rover_position(id) == sim2.get_rover_position(ids2[0]),
		"split stepping matches single-run stepping bit-exactly"
	)

	# Unknown rover: warned, zero vector, world alive.
	check(sim.get_rover_position(999) == Vector2.ZERO, "unknown rover -> ZERO")
	sim.step_ticks(1)
	check(sim.get_tick() == 111, "world ticks after unknown-rover probe")

	sim.queue_free()
	sim2.queue_free()
	_finish()


func check(condition: bool, message: String) -> void:
	if condition:
		print("ok   - ", message)
	else:
		_failures += 1
		push_error("FAIL - " + message)


func _finish() -> void:
	if _failures == 0:
		print("SMOKE OK")
		quit(0)
	else:
		print("SMOKE FAILED: %d check(s)" % _failures)
		quit(1)
