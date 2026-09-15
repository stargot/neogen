# Run/Stop/hot-reload orchestrator (backlog 5.4).
#
# Semantics (documented decisions):
#   - Rover selection is MVP-simple: the first (lowest-id) rover.
#   - Run saves the editor buffer first (IDE convention), then stop+detach
#     the previously bound script and attach fresh (a new script id per
#     run; the editor cares about the file, not the id).
#   - Stop = stop_script + clear the rover's command queue: commands
#     belong to the rover (core contract), so a Finished-but-driving
#     script would otherwise keep moving it. The file binding survives.
#   - Hot-reload is ATTACH-FIRST (review #1): compile the new text; only
#     on success stop+detach the old script. On failure the binding keeps
#     the previous text (the next poll retries) and the player sees a
#     line in the console - a broken auto-save must never freeze the
#     running script silently. Host events reach the console through
#     ConsolePanel.append_host_line (a direct method call, minimal
#     coupling - the log_line signal is for script output only).
#   - Hot-reload polls every second (SceneTree timer; tests call
#     check_now() directly) and only when the editor's auto-restart
#     checkbox is on.
extends Node

const ScriptFiles = preload("res://scripts/script_files.gd")
const POLL_INTERVAL := 1.0

var files := ScriptFiles.new()
var _sim = null
var _editor = null
var _console = null

## Current Run binding: {file: String, rover: int, script_id: int, text: String}.
var binding: Dictionary = {}


func _ready() -> void:
	if _sim == null:
		bind_sim(get_node_or_null("../Sim"))
	_editor = get_node_or_null("../EditorPanel")
	_poll_loop()


# Public for tests: bind any SimNode.
func bind_sim(sim) -> void:
	if sim == null:
		push_warning("Neogen runner: no SimNode to bind")
		return
	_sim = sim


func _poll_loop() -> void:
	while is_inside_tree():
		await get_tree().create_timer(POLL_INTERVAL).timeout
		check_now()


## Run the given file on the first rover. Returns the script id or -1.
func run_file(file_name: String) -> int:
	if _sim == null or not _sim.has_method("get_rover_ids"):
		_host_line("run failed: no simulation bound")
		return -1
	var rover_ids: PackedInt64Array = _sim.get_rover_ids()
	if rover_ids.is_empty():
		_host_line("run failed: no rovers in the world")
		return -1
	var res: Dictionary = files.read_checked(file_name)
	if res.get("err") != OK:
		_host_line("run failed: cannot read %s (%s)" % [file_name, error_string(res.get("err"))])
		return -1
	var text: String = res.get("text")
	var rover: int = rover_ids[0]
	_stop_bound()
	var script_id: int = _sim.attach_script(rover, text)
	if script_id < 0:
		binding = {}
		_host_line("run failed: %s does not compile, nothing is running" % file_name)
		return -1
	binding = {"file": file_name, "rover": rover, "script_id": script_id, "text": text}
	return script_id


## Stop the run: park the script AND clear the rover's command queue -
## commands belong to the rover (core contract), so a finished-but-
## driving script would otherwise keep moving it. The file binding
## survives (hot-reload keeps working after a manual Stop).
func stop() -> bool:
	if not _sim or binding.is_empty():
		return false
	var stopped: bool = _sim.stop_script(binding.get("script_id"))
	var cleared: bool = _sim.clear_rover_commands(binding.get("rover"))
	return stopped or cleared


## One hot-reload poll: re-read the bound file, reload on change (when the
## editor's auto-restart is on). ATTACH-FIRST - see the header. Public
## for tests.
func check_now() -> void:
	if _sim == null or binding.is_empty():
		return
	if _editor != null and not _editor.is_auto_restart():
		return
	var file_name: String = binding.get("file")
	var res: Dictionary = files.read_checked(file_name)
	if res.get("err") != OK:
		return  # real read error (e.g. vanished file): skip, retry next poll
	var text: String = res.get("text")
	if text == binding.get("text"):
		return
	var rover: int = binding.get("rover")
	var new_id: int = _sim.attach_script(rover, text)
	if new_id < 0:
		# Keep the old script running and the binding on the old text;
		# the next poll retries, the player sees why.
		_host_line(
			"hot-reload failed for %s: keeping previous version" % file_name
		)
		return
	_stop_bound()
	binding = {"file": file_name, "rover": rover, "script_id": new_id, "text": text}


func _stop_bound() -> void:
	if _sim == null or binding.is_empty():
		return
	var old_id: int = binding.get("script_id")
	var rover: int = binding.get("rover")
	_sim.stop_script(old_id)
	_sim.detach_script(old_id)
	_sim.clear_rover_commands(rover)


## Host-side line into the game console (direct method call; the log_line
## signal is script output only - see the header).
func _host_line(text: String) -> void:
	push_warning("Neogen runner: %s" % text)
	if _console == null:
		_console = get_node_or_null("../ConsolePanel")
	if _console != null and _console.has_method("append_host_line"):
		_console.append_host_line(text)
