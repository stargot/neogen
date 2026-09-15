# Scene integration test (phase 6 fix-round) — the missing level between
# per-panel smoke tests and a real launch: it boots the REAL
# res://scenes/main.tscn, lets it run, and exercises the wiring in the
# live tree: input toggles, HUD polling, the runner path, the console
# panel. Per-panel tests instantiate nodes programmatically and miss
# exactly this class of breakage (a stale scene reference).
#
# Run from godot/:
#   godot --headless --script res://tests/scene_integration.gd
# Exit 0 + "SCENE INTEGRATION OK" = the assembled scene works end to end.
extends SceneTree

var _failures := 0


func _initialize() -> void:
	# Deferred: the real scene needs a live tree (physics frames).
	_run.call_deferred()


func _run() -> void:
	# 1. Boot the REAL main scene.
	var packed: PackedScene = load("res://scenes/main.tscn")
	var main: Node = packed.instantiate()
	root.add_child(main)
	await process_frame
	await process_frame

	var sim: SimNode = main.get_node("Sim")
	var rover: Node2D = main.get_node("Rover")
	var editor: CanvasLayer = main.get_node("EditorPanel")
	var hud_tick: Label = main.get_node("HUD/Panel/Margin/VBox/TickLabel")
	var console: CanvasLayer = main.get_node("ConsolePanel")
	var runner: Node = main.get_node("Runner")
	var rover_id: int = sim.get_rover_ids()[0]
	var start := sim.get_rover_position(rover_id)

	# 2. HUD: the tick label follows the live simulation.
	var frame := await _poll(func() -> bool: return not hud_tick.text.ends_with(": 0"), 240)
	_check(frame >= 0, "HUD tick label updates in the live scene ('%s')" % hud_tick.text)

	# 3. F12 toggles the editor panel through real input.
	check_visible_editor(editor, true)
	_press_key(KEY_F12)
	frame = await _poll(func() -> bool: return not editor.visible, 60)
	_check(frame >= 0, "F12 hides the editor panel in the live scene")
	_press_key(KEY_F12)
	frame = await _poll(func() -> bool: return editor.visible, 60)
	_check(frame >= 0, "F12 shows the editor panel again")

	# 3b. The editor toolbar buttons are wired (pressed -> handlers).
	var run_button: Button = main.get_node("EditorPanel/Panel/Margin/VBox/Toolbar/RunButton")
	var stop_button: Button = main.get_node("EditorPanel/Panel/Margin/VBox/Toolbar/StopButton")
	_check(run_button.pressed.get_connections().size() > 0, "Run button is connected")
	_check(stop_button.pressed.get_connections().size() > 0, "Stop button is connected")

	# 4. The runner path on the live scene: run the shipped demo.
	# (The editor Run button calls exactly this code; here we drive the
	# runner directly for determinism.)
	var sid: int = runner.run_file("patrol.lua")
	_check(sid > 0, "runner.run_file(patrol.lua) works in the live scene")
	frame = await _poll(
		func() -> bool: return sim.get_rover_position(rover_id).distance_to(start) > 1.0, 240
	)
	_check(frame >= 0, "rover moves in the live scene after Run (frame %d)" % frame)

	# 5. The console panel renders the patrol reports.
	frame = await _poll(
		func() -> bool: return console.get_log_text().contains("патруль"), 240
	)
	_check(frame >= 0, "console panel shows patrol reports in the live scene")

	# 6. Stop freezes the rover.
	runner.stop()
	var frozen := sim.get_rover_position(rover_id)
	frame = await _poll(
		func() -> bool: return sim.get_rover_position(rover_id).distance_to(frozen) > 0.5, 90
	)
	_check(frame == -1, "Stop freezes the rover in the live scene")

	main.queue_free()
	_finish()


func check_visible_editor(editor: CanvasLayer, expected: bool) -> void:
	_check(editor.visible == expected, "editor panel visible == %s" % expected)


func _press_key(keycode: int) -> void:
	var event := InputEventKey.new()
	event.keycode = keycode as Key
	event.physical_keycode = keycode as Key
	event.pressed = true
	Input.parse_input_event(event)
	var release := event.duplicate()
	release.pressed = false
	Input.parse_input_event(release)


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
		print("SCENE INTEGRATION OK")
		quit(0)
	else:
		print("SCENE INTEGRATION FAILED: %d check(s)" % _failures)
		quit(1)
