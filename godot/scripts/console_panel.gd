# Mission console panel (backlog 5.3) — evolution of the minimal 3.4
# console.gd receiver, which it REPLACES: exactly one log_line subscriber
# per SimNode must exist, otherwise every line lands twice.
#
# Toggle: ` (backquote) shows/hides the panel (F12 belongs to the editor).
#
# Line format: "[tick N] rover M: text"; error lines (text starting with
# "error: ") are tinted warm-red via bbcode. Raw text is bracket-escaped
# so player-printed bbcode cannot re-style the log. Font: the default
# theme font (the art pass may swap it for a mono one).
extends CanvasLayer

const ERROR_COLOR := "c96a5a"
const ERROR_PREFIX := "error: "

var _sim = null

## Lazy node resolution: the panel may receive log lines before _ready
## fires (nodes added during SceneTree._initialize defer their ready) -
## the HUD/embedding lesson generalized. Same for the toolbar buttons.
var _log_label: RichTextLabel
var _clear_button: Button
var _copy_button: Button


func _log() -> RichTextLabel:
	if _log_label == null:
		_log_label = get_node("Panel/Margin/VBox/Log")
	return _log_label


func _ready() -> void:
	# Buttons resolve lazily too (same reason as _log): no @onready.
	_clear_button = get_node("Panel/Margin/VBox/Toolbar/ClearButton")
	_copy_button = get_node("Panel/Margin/VBox/Toolbar/CopyButton")
	_clear_button.pressed.connect(clear_log)
	_copy_button.pressed.connect(copy_log)
	# Explicit bind wins over auto-wiring (same lesson as the HUD flake).
	if _sim == null:
		bind_sim(get_node_or_null("../Sim"))


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and event.keycode == KEY_QUOTELEFT:
		visible = not visible
		get_viewport().set_input_as_handled()


# Public for tests: bind any SimNode (the scene wiring uses ../Sim).
# Guards against a double connect (same pattern as hud.gd).
func bind_sim(sim) -> void:
	if sim == null:
		push_warning("Neogen console: no SimNode to bind")
		return
	if _sim != null and _sim == sim:
		return
	_sim = sim
	sim.log_line.connect(_on_log_line)


## Host-side line (runner events - Run/hot-reload failures). Direct
## method call, not the log_line signal: that one carries script output.
func append_host_line(text: String) -> void:
	_log().append_text("[host] %s
" % _escape(text))


func _on_log_line(tick: int, rover_id: int, text: String) -> void:
	_log().append_text(_format_line(tick, rover_id, text) + "\n")


## The rendered line; errors get a bbcode color span. Exposed for tests.
static func _format_line(tick: int, rover_id: int, text: String) -> String:
	var line := "[tick %d] rover %d: %s" % [tick, rover_id, _escape(text)]
	if text.begins_with(ERROR_PREFIX):
		return "[color=#%s]%s[/color]" % [ERROR_COLOR, line]
	return line


static func _escape(text: String) -> String:
	return text.replace("[", "[lb]")


func clear_log() -> void:
	_log().clear()


func copy_log() -> void:
	DisplayServer.clipboard_set(_log().get_parsed_text())


## Parsed (bbcode-free) text of the log, for tests and the Copy button.
## NOTE: RichTextLabel.get_text() stays empty in headless runs - the
## parsed text is the reliable readout.
func get_log_text() -> String:
	return _log().get_parsed_text()


## Raw bbcode of the log (error spans carry the color tag), for tests.
## Same headless caveat as get_log_text - empty until the control lays
## out with a renderer.
func get_raw_bbcode() -> String:
	return _log().text
