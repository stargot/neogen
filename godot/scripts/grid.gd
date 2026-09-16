# Minimal world grid (backlog 4.1): one core unit per cell, drawn as lines
# over a fixed area. Purely visual — the core world is authoritative and
# has no grid of its own yet (tiles arrive in phase 7).
extends Node2D

const CELL := 1.0          # one core world unit
const HALF_EXTENT := 12.0  # grid spans this many units around the origin
const LINE_COLOR := Color(0.93, 0.86, 0.66, 0.09)  # warm, faint (weakened for the ground, 6.5.1)

# The start pad mirrors the simulation core: START_PAD_HALF_SIZE = 8
# (neogen-core generate.rs) -> a 16x16 landing zone, plus a 1-unit visual
# margin. Keep in sync when the core constant changes.
const PAD_HALF_SIZE := 8.0 + 1.0
const PAD_FILL := Color(0.24, 0.31, 0.24, 1.0)
const PAD_EDGE := Color(0.93, 0.86, 0.66, 0.35)


func _ready() -> void:
	queue_redraw()


func _draw() -> void:
	# Start pad: fill + outline from the named constants above.
	var pad_rect := Rect2(
		Vector2(-PAD_HALF_SIZE, -PAD_HALF_SIZE), Vector2(2 * PAD_HALF_SIZE, 2 * PAD_HALF_SIZE)
	)
	draw_rect(pad_rect, PAD_FILL)
	draw_rect(pad_rect, PAD_EDGE, false, 0.05)

	var i := -HALF_EXTENT
	while i <= HALF_EXTENT + 0.001:
		var p := i
		draw_line(Vector2(p, -HALF_EXTENT), Vector2(p, HALF_EXTENT), LINE_COLOR)
		draw_line(Vector2(-HALF_EXTENT, p), Vector2(HALF_EXTENT, p), LINE_COLOR)
		i += CELL
