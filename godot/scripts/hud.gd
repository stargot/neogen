# Minimal HUD (backlog 4.3): tick, seed and script statuses.
#
# Polling model (deliberate, MVP): the tick/seed/status labels refresh in
# `_process` from SimNode getters; only the *last error text* arrives via
# the log_line signal (filtered for "error: " lines). Signal-driven tick
# updates can replace the poll when the UI grows (phase 5).
extends CanvasLayer

const ERROR_PREFIX := "error: "

var last_error := ""

var _sim = null

@onready var tick_label: Label = $Panel/Margin/VBox/TickLabel
@onready var seed_label: Label = $Panel/Margin/VBox/SeedLabel
@onready var status_label: Label = $Panel/Margin/VBox/StatusLabel


func _ready() -> void:
	bind_sim(get_node_or_null("../Sim"))


# Public for tests: bind any SimNode (the default wiring uses ../Sim).
func bind_sim(sim) -> void:
	_sim = sim
	if sim != null:
		sim.log_line.connect(_on_log_line)


func _process(_delta: float) -> void:
	if _sim == null:
		return
	tick_label.text = "tick: %d" % _sim.get_tick()
	seed_label.text = "seed: %d" % _sim.get("seed")

	var parts: Array[String] = []
	for id in _sim.get_script_ids():
		var state: Dictionary = _sim.get_script_state(id)
		parts.append("#%d: %s" % [id, _state_text(state)])
	status_label.text = "scripts: " + (" | ".join(parts) if parts.size() > 0 else "none")


func _state_text(state: Dictionary) -> String:
	match state.get("state"):
		"error":
			# The human-readable part sits at the END of the first line
			# (before the traceback); keep the tail so the cause stays
			# visible in the narrow panel.
			var message: String = str(state.get("error", last_error))
			message = message.split("
")[0]
			if message.length() > 48:
				message = "…" + message.substr(message.length() - 47)
			return "error (%s)" % message
		"running", "suspended", "finished", "stopped":
			return state.get("state")
		_:
			return "unknown"


func _on_log_line(_tick: int, _rover_id: int, text: String) -> void:
	if text.begins_with(ERROR_PREFIX):
		last_error = text
