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
	_log_bridge_checks()
	# The mirror checks need a live tree (physics frames); suspending
	# _initialize itself with await breaks the tree setup, so run the
	# coroutine deferred instead.
	_rover_mirror_checks.call_deferred()


func _log_bridge_checks() -> void:
	# Backlog 3.4: script print -> LogBuffer -> log_line signal.
	var sim := SimNode.new()
	root.add_child(sim)
	var id := sim.get_rover_ids()[0]

	var captured: Array = []
	sim.log_line.connect(
		func(tick: int, rover_id: int, text: String): captured.append([tick, rover_id, text])
	)

	var script_id := sim.attach_script(id, "print(\"hi\")")
	check(script_id > 0, "print script attached, got id %d" % script_id)
	check(captured.is_empty(), "nothing logged before the first tick")

	sim.step_ticks(1)
	check(captured.size() == 1, "one log_line after the tick, got %d" % captured.size())
	check(
		captured.size() == 1
			and captured[0][0] == 0
			and captured[0][1] == id
			and captured[0][2] == "hi",
		"log payload matches (tick, rover, text): %s" % str(captured)
	)

	# Repeat polls never duplicate: the drain is read -> cleared.
	sim.step_ticks(1)
	sim.step_ticks(1)
	check(captured.size() == 1, "no duplicate lines on repeat ticks, got %d" % captured.size())

	# Error lines flow through the same signal (lifecycle 2.5).
	var bad := sim.attach_script(id, "error('boom in log')")
	check(bad > 0, "failing script attached")
	sim.step_ticks(1)
	check(
		captured.size() == 2 and str(captured[1][2]).begins_with("error: "),
		"error line captured: %s" % str(captured)
	)

	# Compile errors do not attach anything and log nothing.
	check(sim.attach_script(id, "return +") == -1, "compile error -> -1")
	sim.step_ticks(1)
	check(captured.size() == 2, "no line for the failed attach")

	# Unknown rover: rejected at attach time, nothing runs (review #2).
	check(
		sim.attach_script(999, "print('ghost')") == -1,
		"print-only script on a fake rover id -> -1"
	)
	sim.step_ticks(1)
	check(captured.size() == 2, "ghost script never logged")
	sim.queue_free()


func _rover_mirror_checks() -> void:
	# Backlog 3.3: a RoverNode mirrors a core rover through the real
	# physics/render loop (accumulator-driven ticks + chase interpolation).
	var sim := SimNode.new()
	root.add_child(sim)
	var id := sim.get_rover_ids()[0]

	var rover_scene: PackedScene = load("res://scenes/rover.tscn")
	var rover: Node2D = rover_scene.instantiate()
	rover.set("rover_id", id)
	rover.set("sim_path", NodePath(sim.get_path()))
	root.add_child(rover)

	# A few frames let the chase close on the core start position.
	for i in range(3):
		await process_frame
	var core_start := sim.get_rover_position(id)
	var initial_gap := Vector2.ZERO.distance_to(core_start)
	var current_gap := rover.position.distance_to(core_start)
	check(
		current_gap < initial_gap,
		"mirror chases the core start position (%.3f < %.3f)" % [current_gap, initial_gap]
	)

	# Known movement through the real loop: 120 physics frames at 60 fps
	# = ~60 ticks at 30 tps — enough for the move and the convergence.
	sim.debug_move_rover(id, 3.0, 4.0)
	for i in range(120):
		await physics_frame
	var target := Vector2(3.0, -4.0)
	var after_ticks: Vector2 = rover.position
	check(
		(after_ticks - target).length() < 0.01,
		"after move + K ticks the mirror is within eps of the target, got %s" % after_ticks
	)

	# Smoothing converges (and does not drift apart) with more frames.
	for i in range(60):
		await physics_frame
	var settled: Vector2 = rover.position
	check(
		(settled - target).length() < 0.001,
		"interpolation converges to the target, got %s" % settled
	)
	check(
		(settled - target).length() <= (after_ticks - target).length(),
		"extra frames do not diverge from the target"
	)
	sim.queue_free()
	rover.queue_free()

	# Mirror with an unknown rover id: inert, no drift (review #3).
	var sim_b := SimNode.new()
	root.add_child(sim_b)
	var ghost_scene: PackedScene = load("res://scenes/rover.tscn")
	var ghost: Node2D = ghost_scene.instantiate()
	ghost.set("rover_id", 999)
	ghost.set("sim_path", NodePath(sim_b.get_path()))
	root.add_child(ghost)
	ghost.position = Vector2(5.0, 5.0)
	check(ghost.call("is_valid") == false, "ghost mirror reports invalid")
	for i in range(10):
		await process_frame
	check(
		ghost.position == Vector2(5.0, 5.0),
		"invalid mirror stays where placed, got %s" % ghost.position
	)
	sim_b.queue_free()
	ghost.queue_free()
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
