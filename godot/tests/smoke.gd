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
	_speed_heading_checks()
	_log_bridge_checks()
	# Async suites run sequentially (editor first, then mirror): two
	# interleaved coroutines shift frame timing and flake the mirror
	# chase checks. Both are deferred - awaiting inside _initialize
	# breaks the tree setup.
	_editor_checks.call_deferred()


func _editor_checks() -> void:
	# Backlog 5.1: editor panel - create, edit, save, list; errors survive.
	var panel: CanvasLayer = load("res://ui/editor_panel.tscn").instantiate()
	root.add_child(panel)
	await process_frame

	var ScriptFiles = load("res://scripts/script_files.gd")
	var files = ScriptFiles.new()
	var probe := "smoke_editor_probe.lua"

	# Clean probe remnants, then create through the panel.
	files.remove(probe)
	panel._on_new_pressed()
	var created: String = panel._current_file
	check(created.ends_with(".lua"), "new file created: %s" % created)

	# setText -> save -> disk matches (full-rewrite MVP).
	var text := "print('from editor panel')
"
	panel.code_edit.text = text
	check(panel.save_current() == OK, "save returns OK")
	check(files.read(created) == text, "disk content matches the editor text")

	# The dropdown lists the created file.
	var listed := false
	for i in panel.file_list.item_count:
		if panel.file_list.get_item_text(i) == created:
			listed = true
	check(listed, "file list shows the created file")

	# Refresh re-scans: file still there after a manual refresh.
	panel.refresh_files(created)
	check(panel._current_file == created, "refresh keeps the selection")
	check(panel.code_edit.text == text, "refresh reloads the disk text")

	# Unsafe names are rejected without crashing anything.
	check(
		files.write("../escape.lua", "x") == ERR_INVALID_PARAMETER,
		"traversal name rejected"
	)
	check(files.read("../escape.lua") == "", "traversal read rejected")
	panel._set_status("probe done")
	check(panel.status.text == "probe done", "status line works")

	# Highlighter attached, keywords mapped to the palette (backlog 5.2).
	var highlighter = panel.code_edit.syntax_highlighter
	check(highlighter != null, "CodeEdit has a syntax highlighter")
	if highlighter != null:
		check(
			highlighter.has_keyword_color("while") and highlighter.has_keyword_color("function"),
			"highlighter knows Lua keywords"
		)
		check(
			highlighter.get_keyword_color("while") == load("res://scripts/lua_highlighter.gd").KEYWORD_COLOR,
			"keyword color comes from the Lua palette"
		)

	# Completion: the player API and keywords are offered by prefix.
	check(panel.code_edit.code_completion_enabled, "code completion enabled")
	var api = load("res://resources/lua_api.json")
	check(
		api is JSON and api.data != null and api.data.get("functions", []).size() == 6,
		"lua_api.json parses with six signatures"
	)
	panel.code_edit.text = "m"
	panel.code_edit.set_caret_column(1)
	panel.code_edit.request_code_completion()
	var displays: Array = []
	for option in panel.code_edit.get_code_completion_options():
		displays.append(option.get("display_text", ""))
	check("move" in displays, "prefix m -> move offered, got %s" % str(displays))
	panel.code_edit.text = "sc"
	panel.code_edit.set_caret_column(2)
	panel.code_edit.request_code_completion()
	displays = []
	for option in panel.code_edit.get_code_completion_options():
		displays.append(option.get("display_text", ""))
	check(
		"scan" in displays and "scan_result" in displays,
		"prefix sc -> scan and scan_result offered, got %s" % str(displays)
	)
	panel.code_edit.text = "whi"
	panel.code_edit.set_caret_column(3)
	panel.code_edit.request_code_completion()
	displays = []
	for option in panel.code_edit.get_code_completion_options():
		displays.append(option.get("display_text", ""))
	check("while" in displays, "Lua keywords complete too, got %s" % str(displays))
	panel.code_edit.text = ""

	files.remove(created)
	panel.refresh_files()
	panel.queue_free()
	await _rover_mirror_checks()


func _speed_heading_checks() -> void:
	# Backlog 4.2: effective-speed and heading getters stay consistent.
	var sim := SimNode.new()
	root.add_child(sim)
	var id := sim.get_rover_ids()[0]

	check(sim.get_rover_speed(id) == 0.0, "parked rover speed is 0")
	var parked_heading: Vector2 = sim.get_rover_heading(id)
	check(
		absf(parked_heading.length() - 1.0) < 0.001,
		"parked heading is a unit vector, got %s" % parked_heading
	)

	sim.debug_move_rover(id, 9.0, 5.0)
	sim.step_ticks(1)
	check(sim.get_rover_speed(id) == 2.0, "moving at default cruise 2.0")
	var pos: Vector2 = sim.get_rover_position(id)
	var heading: Vector2 = sim.get_rover_heading(id)
	var to_target := (Vector2(9.0, -5.0) - pos).normalized()
	check(
		heading.dot(to_target) > 0.99,
		"heading points at the target while driving: %s vs %s" % [heading, to_target]
	)

	sim.step_ticks(100)
	check(sim.get_rover_speed(id) == 0.0, "speed back to 0 after arrival")

	check(sim.get_rover_speed(999) == -1.0, "unknown rover speed -> -1")
	check(sim.get_rover_heading(999) == Vector2.ZERO, "unknown rover heading -> ZERO")
	sim.queue_free()


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

	# The chase must close on the core start position. Robust semantics:
	# wait (bounded) until the mirror is halfway there instead of asserting
	# on an exact frame count - frame/resume ordering between the SceneTree
	# script and node _process is timing-dependent.
	var core_start := sim.get_rover_position(id)
	var initial_gap := Vector2.ZERO.distance_to(core_start)
	var frame := await poll_until(
		func() -> bool: return rover.position.distance_to(core_start) < initial_gap * 0.5
	)
	check(
		frame >= 0,
		"mirror chases the core start position (frame %d, gap %.3f -> %.3f)"
			% [frame, initial_gap, rover.position.distance_to(core_start)]
	)

	# Known movement through the real loop: 120 physics frames at 60 fps
	# = ~60 ticks at 30 tps — enough for the move and the convergence.
	sim.debug_move_rover(id, 3.0, 4.0)
	var target := Vector2(3.0, -4.0)
	frame = await poll_until(
		func() -> bool: return rover.position.distance_to(target) < 0.1, 240
	)
	check(
		frame >= 0,
		"mirror reaches the move target within eps (frame %d), got %s"
			% [frame, rover.position]
	)

	# Smoothing converges (and does not drift apart) with more frames.
	frame = await poll_until(
		func() -> bool: return rover.position.distance_to(target) < 0.01, 120
	)
	check(
		frame >= 0,
		"interpolation converges to the target (frame %d), got %s"
			% [frame, rover.position]
	)
	# No divergence: ten more frames must not move the mirror away from
	# the converged distance (small slack for the last chase steps).
	var converged_gap: float = rover.position.distance_to(target)
	for i in range(10):
		await process_frame
	check(
		rover.position.distance_to(target) <= maxf(converged_gap * 2.0, 0.02),
		"extra frames do not diverge from the target (%.4f -> %.4f)"
			% [converged_gap, rover.position.distance_to(target)]
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
	await _hud_checks()


func _hud_checks() -> void:
	# Backlog 4.3: HUD exists, tracks ticks, and shows script status.
	var sim := SimNode.new()
	root.add_child(sim)
	var id := sim.get_rover_ids()[0]

	# Freeze this sim's automatic ticking: while we poll process frames,
	# physics_process keeps advancing the accumulator and the tick label
	# would legitimately race past the expected value. step_ticks works
	# regardless of the processing mode.
	sim.process_mode = Node.PROCESS_MODE_DISABLED

	var hud_scene: PackedScene = load("res://ui/hud.tscn")
	var hud: CanvasLayer = hud_scene.instantiate()
	# Bind BEFORE add_child: in a headless SceneTree script, _ready of
	# added nodes fires on the first frame - a later auto-bind to the
	# scene's own Sim would race this explicit one (that was the HUD
	# flake: the label froze at the main Sim's tick). hud.gd also guards
	# with "explicit bind wins", making the order deterministic either
	# way.
	hud.bind_sim(sim)
	root.add_child(hud)

	var tick_label: Label = hud.get_node("Panel/Margin/VBox/TickLabel")
	var seed_label: Label = hud.get_node("Panel/Margin/VBox/SeedLabel")
	var status_label: Label = hud.get_node("Panel/Margin/VBox/StatusLabel")

	var frame := await poll_until(func() -> bool: return seed_label.text == "seed: 42")
	check(frame >= 0, "HUD shows the seed (frame %d), got %s" % [frame, seed_label.text])
	sim.step_ticks(7)
	frame = await poll_until(func() -> bool: return tick_label.text == "tick: 7")
	check(
		frame >= 0,
		"HUD tick label follows the simulation (frame %d), got %s" % [frame, tick_label.text]
	)
	frame = await poll_until(func() -> bool: return status_label.text == "scripts: none")
	check(
		frame >= 0,
		"no scripts attached -> none (frame %d), got %s" % [frame, status_label.text]
	)

	# A failing script flips the status to error with the message.
	var sid := sim.attach_script(id, "error('hud boom')")
	check(sid > 0, "failing script attached")
	sim.step_ticks(1)
	frame = await poll_until(
		func() -> bool:
			var text: String = status_label.text
			return text.begins_with("scripts: #1: error (") and text.contains("hud boom")
	)
	check(
		frame >= 0,
		"HUD shows the error status with the text (frame %d), got %s"
			% [frame, status_label.text]
	)
	check(
		hud.last_error.contains("hud boom"),
		"HUD captured the last error via log_line"
	)

	sim.queue_free()
	hud.queue_free()
	_finish()


## Poll until the predicate holds, at most `budget` frames; returns the
## frame index when it held, or -1. UI-bound values (HUD labels, mirrors)
## update in node _process - never assert them after a fixed frame count.
func poll_until(predicate: Callable, budget: int = 60) -> int:
	for i in range(budget):
		if predicate.call():
			return i
		await process_frame
	return -1


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
