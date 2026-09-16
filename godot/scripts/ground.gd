# Ground renderer (backlog 6.5.1): one _draw pass over a fixed window of
# seeded cells around the origin (one cell = one core unit). The start
# pad (grid.gd, PAD_HALF_SIZE) draws on top; grid lines stay, weakened.
#
# Seed source: the SimNode via the explicit sim_path export - same
# pattern as hud.gd (read once; warn once if the Sim is missing, then
# stay quiet - the ground is decorative, not gameplay-critical).
#
# Lives inside the World scene, so the default path is ../../Sim
# (World/Ground -> Main/Sim).
extends Node2D

const GroundGen = preload("res://scripts/ground_gen.gd")

const HALF_CELLS := 32  # 64x64 cells around the origin - covers max zoom

@export var sim_path: NodePath = ^"../../Sim"

var _seed := 0
var _seeded := false


func _ready() -> void:
	var sim = get_node_or_null(sim_path)
	if sim == null:
		push_warning("ground: no SimNode at %s - drawing with seed 0" % sim_path)
	else:
		_seed = int(sim.get("seed"))
		_seeded = true
	queue_redraw()


func _draw() -> void:
	for y in range(-HALF_CELLS, HALF_CELLS):
		for x in range(-HALF_CELLS, HALF_CELLS):
			draw_rect(
				Rect2(x, y, 1.0, 1.0),
				GroundGen.cell_color(_seed, x, y)
			)
	# A soft frame so the edge of the drawn window does not read as a wall.
	var edge := Rect2(-HALF_CELLS, -HALF_CELLS, 2 * HALF_CELLS, 2 * HALF_CELLS)
	draw_rect(edge, Color(0, 0, 0, 0.0), false, 0.0)
