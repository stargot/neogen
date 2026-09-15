# Minimal console receiver (backlog 3.4): subscribes to SimNode.log_line,
# prints every entry and keeps it in `lines` for tests/UI.
extends Node

var lines: Array = []


func _ready() -> void:
	var sim := get_node_or_null("../Sim")
	if sim == null:
		push_warning("Neogen console: no SimNode at ../Sim")
		return
	sim.log_line.connect(_on_log_line)


func _on_log_line(tick: int, rover_id: int, text: String) -> void:
	lines.append({"tick": tick, "rover_id": rover_id, "text": text})
	print("[neogen] tick %d rover %d: %s" % [tick, rover_id, text])
