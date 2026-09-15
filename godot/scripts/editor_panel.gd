# Editor dock panel (backlog 5.1): open/create/save player Lua scripts.
#
# Toggle: F12 shows/hides the panel (also documented in README).
#
# Write model: FULL REWRITE of the selected file on save (MVP; no diffs).
# Path model: the dev-build scripts folder is resolved once via
# ProjectSettings.globalize_path (see script_files.gd for the export
# limitation).
extends CanvasLayer

const ScriptFiles = preload("res://scripts/script_files.gd")
const LuaHighlighter = preload("res://scripts/lua_highlighter.gd")
const LUA_API_PATH := "res://resources/lua_api.json"

var _files := ScriptFiles.new()
var _current_file := ""

@onready var file_list: OptionButton = $Panel/Margin/VBox/Toolbar/FileList
@onready var code_edit: CodeEdit = $Panel/Margin/VBox/CodeEdit
@onready var status: Label = $Panel/Margin/VBox/Status
@onready var new_button: Button = $Panel/Margin/VBox/Toolbar/NewButton
@onready var save_button: Button = $Panel/Margin/VBox/Toolbar/SaveButton
@onready var refresh_button: Button = $Panel/Margin/VBox/Toolbar/RefreshButton
@onready var run_button: Button = $Panel/Margin/VBox/Toolbar/RunButton
@onready var stop_button: Button = $Panel/Margin/VBox/Toolbar/StopButton

## Runner orchestrator (Run/Stop/hot-reload, backlog 5.4); resolved
## lazily - see the console panel note about _initialize contexts.
var _runner = null


func _ready() -> void:
	new_button.pressed.connect(_on_new_pressed)
	save_button.pressed.connect(_on_save_pressed)
	refresh_button.pressed.connect(_on_refresh_pressed)
	run_button.pressed.connect(_on_run_pressed)
	stop_button.pressed.connect(_on_stop_pressed)
	file_list.item_selected.connect(_on_file_selected)

	# Syntax highlighting (backlog 5.2): warm Lua palette.
	code_edit.syntax_highlighter = LuaHighlighter.new()

	# Code completion: player API signatures from resources/lua_api.json
	# plus the Lua keywords; CodeEdit filters by the typed prefix.
	code_edit.code_completion_enabled = true
	code_edit.code_completion_requested.connect(_on_completion_requested)
	_load_completions()

	refresh_files()


## Completion options populated once at ready (API names + keywords).
var _completion_options: Array = []


func _load_completions() -> void:
	_completion_options.clear()
	var api = load(LUA_API_PATH)
	if api is JSON and api.data != null:
		for entry in api.data.get("functions", []):
			var signature: String = entry.get("signature", entry.get("name", ""))
			var display: String = entry.get("name", signature)
			# Multi-part names (coroutine.yield) complete on the last part.
			if display.contains("."):
				display = display.get_slice(".", 1)
			_completion_options.append([CodeEdit.KIND_FUNCTION, display, display])
	for word in LuaHighlighter.KEYWORDS:
		_completion_options.append([CodeEdit.KIND_PLAIN_TEXT, word, word])


func _on_completion_requested() -> void:
	for option in _completion_options:
		code_edit.add_code_completion_option(option[0], option[1], option[2])
	code_edit.update_code_completion_options(false)


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and event.keycode == KEY_F12:
		visible = not visible
		get_viewport().set_input_as_handled()


## Rebuild the file dropdown from the scripts folder.
func refresh_files(select: String = _current_file) -> void:
	file_list.clear()
	for name in _files.scan():
		file_list.add_item(name)
	if select != "":
		for i in file_list.item_count:
			if file_list.get_item_text(i) == select:
				file_list.select(i)
				_on_file_selected(i)
				return
	_current_file = ""
	_set_status("")


func _on_file_selected(index: int) -> void:
	_current_file = file_list.get_item_text(index)
	code_edit.text = _files.read(_current_file)
	_set_status("open: %s" % _current_file)


func _on_new_pressed() -> void:
	var name := _files.unique_name("script")
	var err := _files.write(name, "")
	if err != OK:
		_set_status("create failed: %s" % error_string(err))
		return
	refresh_files(name)
	_set_status("created: %s" % name)


func _on_save_pressed() -> void:
	save_current()


## Write the editor text into the current file (public for tests).
func save_current() -> int:
	if _current_file == "":
		_set_status("nothing to save: select or create a file first")
		return ERR_DOES_NOT_EXIST
	var err := _files.write(_current_file, code_edit.text)
	_set_status("saved: %s" % _current_file if err == OK else "save failed: %s" % error_string(err))
	return err


func _on_refresh_pressed() -> void:
	refresh_files()


func _runner_node():
	if _runner == null:
		_runner = get_node_or_null("../Runner")
	return _runner


## Run the current file on the rover (runner semantics: stop+detach the
## previous run, attach fresh).
func _on_run_pressed() -> void:
	if _current_file == "":
		_set_status("run: select or create a file first")
		return
	var runner = _runner_node()
	if runner == null:
		_set_status("run: no Runner node at ../Runner")
		return
	# IDE convention (review #5): save the buffer before running, so the
	# run always executes what the player sees in the editor.
	save_current()
	var script_id: int = runner.run_file(_current_file)
	if script_id >= 0:
		_set_status("run: %s (script %d)" % [_current_file, script_id])
	else:
		_set_status("run failed: %s" % _current_file)


func _on_stop_pressed() -> void:
	var runner = _runner_node()
	if runner == null:
		_set_status("stop: no Runner node")
		return
	if runner.stop():
		_set_status("stopped")
	else:
		_set_status("stop: nothing bound")


## Auto-restart toggle for hot-reload (single source of truth for the
## runner; public for tests).
func is_auto_restart() -> bool:
	var box: CheckBox = get_node_or_null("Panel/Margin/VBox/Toolbar/AutoRestart")
	return box != null and box.button_pressed


func _set_status(text: String) -> void:
	status.text = text
