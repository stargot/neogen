# Start settlement (backlog 6.5.4): a small living cluster - glass dome,
# tilted solar panels around it, a beacon mast slightly apart. Static
# polygons, no external assets.
#
# Placement is a PURE deterministic function of the core seed (the same
# hash approach as ground_gen): a "residential cluster" on a ring outside
# the start pad (r ~13.5..16.5 u), never on the patrol track (the track
# square sits inside the pad zone, but the keep-out is asserted anyway)
# and never on a rock/crystal cell (reads ground_gen occupancy; occupied
# candidates rotate away).
#
# Shadows (the plan's ASSUMPTION): every structure carries a
# LightOccluder2D so WarmLight casts directional shadows; IN ADDITION a
# subtle contact ellipse is drawn under each structure so they sit on the
# ground even where the occluder shadow reads weakly (headless CI cannot
# verify the visual - the final look is judged by eye in 6.5.7).
extends Node2D

const GroundGen = preload("res://scripts/ground_gen.gd")

const DOME_RING_MIN := 13.5
const DOME_RING_MAX := 16.5
const PANEL_RING_MIN := 2.4
const PANEL_RING_MAX := 3.6
const MAST_DIST_MIN := 3.4
const MAST_DIST_MAX := 4.6

enum Kind { DOME, PANEL, MAST }


func _ready() -> void:
	var sim = get_node_or_null("../Sim")
	if sim == null:
		push_warning("settlement: no SimNode at ../Sim - placing with seed 0")
	else:
		set_meta("seed", int(sim.get("seed")))
	_build_occluders()
	queue_redraw()


func _process(_delta: float) -> void:
	# The mast beacon pulses softly; the cluster is tiny, so a full redraw
	# per frame is cheap.
	queue_redraw()


func _draw() -> void:
	var seed_value: int = int(get_meta("seed", 0))
	for structure in layout(seed_value):
		match structure.kind:
			Kind.DOME:
				_draw_dome(structure)
			Kind.PANEL:
				_draw_panel(structure)
			Kind.MAST:
				_draw_mast(structure)


# --- Pure placement (tested headless without a scene) ----------------

## The cluster layout: a pure function of the seed. 4-5 structures.
static func layout(seed: int) -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	var h := GroundGen.cell_hash(seed ^ 0x5EED, 11, 7)
	var angle: float = float(h & 0x3FF) / 1024.0 * TAU
	var ring: float = DOME_RING_MIN + float((h >> 10) & 0x3FF) / 1024.0 * (DOME_RING_MAX - DOME_RING_MIN)
	var dome := Vector2(cos(angle), sin(angle)) * ring
	out.push_back({
		"kind": Kind.DOME,
		"pos": dome,
		"rot": (float((h >> 20) & 0x3F) / 64.0 - 0.5) * 0.4,
		"scale": 1.0,
		"variant": (h >> 26) & 0x3,
	})

	# 2-3 panels fanned around the dome, on free cells.
	var panels := 2 + ((h >> 30) & 1)
	var base := angle + PI * 0.55
	for i in panels:
		var ph := GroundGen.cell_hash(seed ^ 0x9A1E, i * 31 + 3, i * 17 + 5)
		var pa: float = base + float(i) * (TAU / 7.0)
		var pd: float = PANEL_RING_MIN + float(ph & 0x3FF) / 1024.0 * (PANEL_RING_MAX - PANEL_RING_MIN)
		var pos := dome + Vector2(cos(pa), sin(pa)) * pd
		var tries := 0
		while tries < 4 and _on_blocking_prop(seed, pos):
			pa += 0.7
			pos = dome + Vector2(cos(pa), sin(pa)) * pd
			tries += 1
		out.push_back({
			"kind": Kind.PANEL,
			"pos": pos,
			"rot": pa * 0.5 + float((ph >> 10) & 0x3F) / 64.0,
			"scale": 0.9 + float((ph >> 16) & 0xFF) / 255.0 * 0.3,
			"variant": i,
		})

	# The mast stands a bit apart, on the opposite side of the panel fan.
	var mh := GroundGen.cell_hash(seed ^ 0xABE, 5, 9)
	var ma: float = angle - PI * 0.5
	var md: float = MAST_DIST_MIN + float(mh & 0x3FF) / 1024.0 * (MAST_DIST_MAX - MAST_DIST_MIN)
	var mast := dome + Vector2(cos(ma), sin(ma)) * md
	var mast_tries := 0
	while mast_tries < 4 and _on_blocking_prop(seed, mast):
		ma += 0.9
		mast = dome + Vector2(cos(ma), sin(ma)) * md
		mast_tries += 1
	out.push_back({
		"kind": Kind.MAST,
		"pos": mast,
		"rot": 0.0,
		"scale": 1.0,
		"variant": (mh >> 10) & 0x3,
	})
	return out


## Digest of the placement for the determinism test. Pure.
static func layout_hash(seed: int) -> int:
	var h: int = 0xcbf29ce484222325
	for s in layout(seed):
		var fold: int = s.kind * 7919
		fold += int((s.pos.x + 64.0) * 32.0) * 31
		fold += int((s.pos.y + 64.0) * 32.0) * 17
		fold += int(s.rot * 256.0) + int(s.scale * 128.0)
		h = (h * 0x100000001b3) ^ fold
	return h


static func _on_blocking_prop(seed: int, pos: Vector2) -> bool:
	var kind := GroundGen.prop(seed, floori(pos.x), floori(pos.y))
	return kind == GroundGen.Prop.ROCK or kind == GroundGen.Prop.CRYSTAL


# --- Rendering -------------------------------------------------------

func _draw_dome(s: Dictionary) -> void:
	var center: Vector2 = s.pos
	var radius: float = 1.35 * float(s.scale)
	# Contact shadow grounds the dome regardless of occluder strength.
	_draw_contact_shadow(center, radius * 1.1)
	# Silhouette: a semicircle over a short base.
	var points := PackedVector2Array()
	var steps := 14
	for i in steps + 1:
		var t := PI + float(i) / float(steps) * PI
		points.push_back(center + Vector2(cos(t), sin(t)) * radius)
	points.push_back(center + Vector2(radius, radius * 0.16))
	points.push_back(center + Vector2(-radius, radius * 0.16))
	draw_colored_polygon(points, Color(0.72, 0.9, 0.86, 0.5))
	# Inner warm glow: the dome is alive inside.
	draw_circle(center + Vector2(0, radius * 0.1), radius * 0.62, Color(1.0, 0.84, 0.55, 0.16))
	# Glint arc on the light side (upper-left, toward the 30-deg light).
	var glint := PackedVector2Array()
	for i in 7:
		var t := PI * 1.15 + float(i) / 6.0 * PI * 0.35
		glint.push_back(center + Vector2(cos(t), sin(t)) * (radius * 0.94))
	draw_polyline(glint, Color(0.98, 0.99, 0.92, 0.7), 0.06, true)
	# Base line.
	draw_line(
		center + Vector2(-radius, radius * 0.16),
		center + Vector2(radius, radius * 0.16),
		Color(0.4, 0.38, 0.32, 0.9),
		0.08
	)


func _draw_panel(s: Dictionary) -> void:
	var center: Vector2 = s.pos
	var scale: float = float(s.scale)
	_draw_contact_shadow(center, 0.9 * scale)
	# Tilted panel: a parallelogram reads as a plane catching light.
	var tilt := Vector2(0.35, -0.18) * scale
	var w := 1.5 * scale
	var h := 0.95 * scale
	var body := PackedVector2Array([
		center + Vector2(-w, -h) + tilt,
		center + Vector2(w, -h) + tilt,
		center + Vector2(w, h) - tilt,
		center + Vector2(-w, h) - tilt,
	])
	draw_colored_polygon(body, Color(0.30, 0.42, 0.50, 0.95))
	# Cell grid: warm lines dividing the plane.
	for i in 3:
		var t := -w + (i + 1) * (2.0 * w / 4.0)
		draw_line(
			center + Vector2(t, -h) + tilt, center + Vector2(t, h) - tilt,
			Color(0.85, 0.75, 0.55, 0.6), 0.035
		)
	draw_line(
		center + Vector2(-w, 0) + tilt * 0.2, center + Vector2(w, 0) - tilt * 0.2,
		Color(0.85, 0.75, 0.55, 0.6), 0.035
	)
	# Warm glare streak along the tilt.
	draw_line(
		center + Vector2(-w * 0.7, -h * 0.5) + tilt,
		center + Vector2(w * 0.2, -h * 0.5) + tilt,
		Color(1.0, 0.9, 0.65, 0.55), 0.06
	)
	# Leg.
	draw_line(center + tilt * 0.5, center - tilt * 1.6, Color(0.35, 0.3, 0.22, 0.9), 0.07)


func _draw_mast(s: Dictionary) -> void:
	var base: Vector2 = s.pos
	var variant: int = int(s.variant)
	_draw_contact_shadow(base, 0.5)
	var top := base + Vector2(0, -1.9)
	draw_line(base, top, Color(0.35, 0.3, 0.22, 1.0), 0.09)
	# Cross-arm antenna.
	draw_line(top + Vector2(-0.35, 0.25), top + Vector2(0.35, 0.1), Color(0.35, 0.3, 0.22, 1.0), 0.05)
	# Beacon: soft warm pulse.
	var pulse := 0.55 + 0.45 * sin(Time.get_ticks_msec() * 0.004 + s.variant)
	draw_circle(top, 0.16, Color(1.0, 0.62, 0.35, 0.9))
	draw_circle(top, 0.3, Color(1.0, 0.62, 0.35, 0.25 * pulse))
	draw_circle(top, 0.46, Color(1.0, 0.62, 0.35, 0.10 * pulse))


func _draw_contact_shadow(center: Vector2, radius: float) -> void:
	# Soft ellipse toward the shadow side of the 30-deg light.
	var offset := Vector2(0.35, 0.5) * radius * 0.4
	var points := PackedVector2Array()
	for i in 12:
		var t := float(i) / 12.0 * TAU
		points.push_back(
			center + offset + Vector2(cos(t) * radius, sin(t) * radius * 0.45)
		)
	draw_colored_polygon(points, Color(0.05, 0.06, 0.04, 0.18))


## Directional shadow casters: one LightOccluder2D per structure.
func _build_occluders() -> void:
	var seed_value: int = int(get_meta("seed", 0))
	for s in layout(seed_value):
		var occluder := LightOccluder2D.new()
		var poly := OccluderPolygon2D.new()
		match s.kind:
			Kind.DOME:
				poly.polygon = PackedVector2Array([
					Vector2(-1.3, 0.1), Vector2(0, -1.4), Vector2(1.3, 0.1),
				])
			Kind.PANEL:
				poly.polygon = PackedVector2Array([
					Vector2(-1.5, -0.9), Vector2(1.5, -0.9),
					Vector2(1.5, 0.9), Vector2(-1.5, 0.9),
				])
			Kind.MAST:
				poly.polygon = PackedVector2Array([
					Vector2(-0.12, 0), Vector2(0, -1.9), Vector2(0.12, 0),
				])
		occluder.occluder = poly
		occluder.position = s.pos
		occluder.rotation = s.rot
		add_child(occluder)
