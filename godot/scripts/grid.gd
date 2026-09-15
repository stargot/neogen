# Minimal world grid (backlog 4.1): one core unit per cell, drawn as lines
# over a fixed area. Purely visual — the core world is authoritative and
# has no grid of its own yet (tiles arrive in phase 7).
extends Node2D

const CELL := 1.0          # one core world unit
const HALF_EXTENT := 12.0  # grid spans this many units around the origin
const LINE_COLOR := Color(0.93, 0.86, 0.66, 0.16)  # warm, faint


func _ready() -> void:
	queue_redraw()


func _draw() -> void:
	var i := -HALF_EXTENT
	while i <= HALF_EXTENT + 0.001:
		var p := i
		draw_line(Vector2(p, -HALF_EXTENT), Vector2(p, HALF_EXTENT), LINE_COLOR)
		draw_line(Vector2(-HALF_EXTENT, p), Vector2(HALF_EXTENT, p), LINE_COLOR)
		i += CELL
