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

const HALF_CELLS := 48  # 96x96 cells around the origin - covers the zoom range incl. MIN_ZOOM (6.5.7)

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
	# Props (6.5.3) in a second pass, on top of the base cells.
	for y in range(-HALF_CELLS, HALF_CELLS):
		for x in range(-HALF_CELLS, HALF_CELLS):
			_draw_prop(x, y)
	draw_set_transform(Vector2.ZERO, 0.0, Vector2.ONE)


func _draw_prop(x: int, y: int) -> void:
	var kind := GroundGen.prop(_seed, x, y)
	if kind == GroundGen.Prop.NONE:
		return
	var params := GroundGen.prop_params(_seed, x, y)
	var center := Vector2(x + 0.5 + params.ox, y + 0.5 + params.oy)
	var scale: float = params.scale
	draw_set_transform(center, params.rot, Vector2(scale, scale))
	match kind:
		GroundGen.Prop.GRASS:
			_draw_grass(params.variant)
		GroundGen.Prop.ROCK:
			_draw_rock(params.variant)
		GroundGen.Prop.CRYSTAL:
			_draw_crystal(params.variant)


## A curled fern-like spiral: solarpank "engineered grass".
func _draw_grass(variant: int) -> void:
	var color := Color(0.62, 0.78, 0.45, 0.95)
	if variant == 1:
		color = Color(0.55, 0.74, 0.40, 0.95)
	var points := PackedVector2Array()
	var turns := 2.2 + float(variant) * 0.2
	var steps := 16
	for i in steps:
		var t := float(i) / float(steps - 1) * turns * TAU
		var r := 0.04 + 0.052 * (t / (turns * TAU)) * turns
		points.push_back(Vector2(cos(t), sin(t)) * r)
	draw_polyline(points, color, 0.035, true)
	# A short companion blade.
	draw_line(Vector2(-0.22, 0.0), Vector2(-0.30, -0.16), color, 0.03)


## A faceted boulder: grey body + a lit face toward the warm light.
func _draw_rock(variant: int) -> void:
	var body := PackedVector2Array([
		Vector2(-0.28, -0.12), Vector2(-0.10, -0.27), Vector2(0.17, -0.21),
		Vector2(0.29, 0.03), Vector2(0.10, 0.25), Vector2(-0.23, 0.16),
	])
	if variant == 1:
		body = PackedVector2Array([
			Vector2(-0.25, -0.18), Vector2(0.04, -0.28), Vector2(0.27, -0.08),
			Vector2(0.20, 0.20), Vector2(-0.12, 0.26), Vector2(-0.28, 0.05),
		])
	draw_colored_polygon(body, Color(0.44, 0.47, 0.49, 1.0))
	# Lit facet on the upper-left (the WarmLight shines from 30 deg).
	draw_colored_polygon(
		PackedVector2Array([
			Vector2(-0.10, -0.27), Vector2(0.17, -0.21), Vector2(0.02, -0.06),
			Vector2(-0.20, -0.11),
		]),
		Color(0.62, 0.65, 0.66, 1.0)
	)


## A warm translucent crystal shard with a soft glow and a glint.
func _draw_crystal(variant: int) -> void:
	# Soft glow behind the shard.
	draw_circle(Vector2.ZERO, 0.42, Color(1.0, 0.82, 0.55, 0.10))
	var main := PackedVector2Array([
		Vector2(0.0, -0.34), Vector2(0.10, -0.05), Vector2(0.03, 0.21),
		Vector2(-0.07, 0.18),
	])
	draw_colored_polygon(main, Color(0.78, 0.93, 0.88, 0.62))
	var side := PackedVector2Array([
		Vector2(0.08, 0.02), Vector2(0.24, 0.16), Vector2(0.12, 0.24),
		Vector2(0.04, 0.16),
	])
	if variant != 0:
		draw_colored_polygon(side, Color(0.70, 0.88, 0.82, 0.5))
	# Glint.
	draw_line(Vector2(-0.02, -0.27), Vector2(0.04, -0.07), Color(1.0, 0.98, 0.9, 0.75), 0.03)
