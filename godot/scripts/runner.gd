# Run/Stop/hot-reload orchestrator (backlog 5.4).
#
# Semantics (documented decisions):
#   - Rover selection is MVP-simple: the first (lowest-id) rover.
#   - Run = stop+detach the previously bound script, then attach fresh
#     (a new script id per run; restart_script's keep-id semantics is not
#     used - the editor cares about the file, not the id).
#   - Stop = stop_script on the bound id, but the FILE binding survives:
#     hot-reload keeps working after a manual Stop.
#   - Hot-reload: a 1s poll (SceneTree timer; tests call check_now()
#     directly) re-reads the bound file; on a text change it stops+detaches
#     the old script and re-attaches - but only when the editor's
#     auto-restart checkbox is on (default). Files without a binding are
#     the editor panel's business, not the runner's.
#   - A broken edit stops the old script and fails to attach: the binding
#     is cleared and a warning logged (no silent zombie).
extends Node

const ScriptFiles = preload("res://scripts/script_files.gd")
const POLL_INTERVAL := 1.0

var files := ScriptFiles.new()
var _sim = null
var _editor = null

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
	if _sim == null:
		push_warning("Neogen runner: no SimNode bound")
		return -1
	var rover_ids: PackedInt64Array = _sim.get_rover_ids()
	if rover_ids.is_empty():
		push_warning("Neogen runner: no rovers in the world")
		return -1
	var rover: int = rover_ids[0]
	var text := files.read(file_name)
	if text == "" and not files.valid_name(file_name):
		push_warning("Neogen runner: bad file name %s" % file_name)
		return -1
	_stop_bound()
	var script_id: int = _sim.attach_script(rover, text)
	if script_id < 0:
		binding = {}
		push_warning("Neogen runner: attach failed for %s" % file_name)
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
## editor's auto-restart is on). Public for tests.
func check_now() -> void:
	if _sim == null or binding.is_empty():
		return
	if _editor != null and not _editor.is_auto_restart():
		return
	var file_name: String = binding.get("file")
	var text := files.read(file_name)
	if text == "" or text == binding.get("text"):
		return
	_stop_bound()
	var rover: int = binding.get("rover")
	var script_id: int = _sim.attach_script(rover, text)
	if script_id < 0:
		push_warning(
			"Neogen runner: hot-reload attach failed for %s (broken edit?)" % file_name
		)
		binding = {}
		return
	binding = {"file": file_name, "rover": rover, "script_id": script_id, "text": text}


func _stop_bound() -> void:
	if _sim == null or binding.is_empty():
		return
	var old_id: int = binding.get("script_id")
	var rover: int = binding.get("rover")
	_sim.stop_script(old_id)
	_sim.detach_script(old_id)
	_sim.clear_rover_commands(rover)
