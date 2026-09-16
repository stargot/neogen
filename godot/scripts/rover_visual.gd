# Visual body of a rover (backlog 4.2): direction, squash, trail.
#
# The Rust RoverNode parent keeps the position authority (chase lerp from
# 3.3); this script only reads the sim and shapes the presentation:
#   - rotation: the hull eases toward the core heading (already in screen
#     coordinates via the shared coords rule);
#   - squash: a light stretch-along-motion / squeeze-across response,
#     scaled by the effective speed (0 when parked, DEFAULT_CRUISE_SPEED
#     at full cruise);
#   - trail: a fading Line2D of the last N world positions (top_level, so
#     points are stored in world space and stay behind as the rover
#     drives on).
extends Node2D

const TRAIL_MAX_POINTS := 48
const TRAIL_MIN_STEP := 0.03   # world units between recorded points
const ROTATION_EASE := 0.25
const CRUISE_FOR_FULL_SQUASH := 2.0   # core DEFAULT_CRUISE_SPEED

# --- 6.5.5: hover, wheels, glow trail, warm light ---
# Hover: a small vertical breathing of the BODY transform (a purely
# visual offset - the RoverNode position stays the chase-lerp authority
# from 3.3). Wheels rotate from the effective speed (no fake spin).
# Trail glow: a second, wider semi-transparent Line2D sharing the core
# trail's points (Line2D over a shader: simpler, headless-safe, no
# material pass - a shader can refine this later).
const HOVER_RATE := 1.6               # breathing cycles per second
const HOVER_AMP := 0.04               # world units, deliberately light
const WHEEL_RADIUS := 0.11
const WHEEL_COLOR := Color(0.30, 0.26, 0.20, 1.0)
const WHEEL_LUG_COLOR := Color(0.62, 0.58, 0.48, 1.0)
const WHEEL_OFFSETS := [
	Vector2(-0.24, -0.28), Vector2(0.16, -0.28),
	Vector2(-0.24, 0.28), Vector2(0.16, 0.28),
]
const LIGHT_COLOR := Color(1.0, 0.85, 0.6)
const LIGHT_ENERGY := 0.6
const LIGHT_SPAN := 0.07               # 64px texture * scale -> ~4.5u

var _sim = null
var _rover_id: int = 1
var _trail: Line2D
var _trail_glow: Line2D
var _wheels: Array = []


func _ready() -> void:
	var parent := get_parent()
	_rover_id = parent.get("rover_id")
	_trail = get_node("../Trail")
	# The sim_path export belongs to the RoverNode parent and is relative
	# TO IT ("../Sim" = sibling of the rover in the main scene) - resolve
	# through the parent, not from this Body node.
	var sim_path: NodePath = parent.get("sim_path")
	_sim = parent.get_node_or_null(sim_path)
	if _sim == null:
		push_warning("rover_visual: no SimNode at %s" % sim_path)


func _process(delta: float) -> void:
	if _sim == null:
		return
	var speed: float = _sim.get_rover_speed(_rover_id)
	var heading: Vector2 = _sim.get_rover_heading(_rover_id)

	# Direction: ease toward the core heading.
	if heading.length_squared() > 0.0001:
		rotation = lerp_angle(rotation, heading.angle(), ROTATION_EASE)

	# Squash: stretch along local X (forward), squeeze across.
	var k := clampf(speed / CRUISE_FOR_FULL_SQUASH, 0.0, 1.0)
	scale = Vector2(1.0 + 0.10 * k, 1.0 - 0.14 * k)

	# Hover: breathe the BODY (visual offset only - the node position
	# stays the chase-lerp authority from 3.3).
	position = Vector2(0.0, sin(Time.get_ticks_msec() * 0.001 * HOVER_RATE) * HOVER_AMP)

	# Wheels roll with the effective speed (parked = no fake spin).
	var roll: float = speed * delta / WHEEL_RADIUS
	for wheel in _wheels:
		wheel.rotation += roll

	_update_trail()


func _update_trail() -> void:
	var here: Vector2 = get_parent().global_position
	var points := _trail.points
	if points.size() > 0 and here.distance_to(points[points.size() - 1]) < TRAIL_MIN_STEP:
		return
	_trail.add_point(here)
	if points.size() + 1 > TRAIL_MAX_POINTS:
		_trail.remove_point(0)
	if _trail_glow != null:
		# The glow shares the core trail's points (assignment copies the
		# packed array); it is wider, fainter and warm.
		_trail_glow.points = _trail.points


## Wheels: a circle + two lugs per node, so the rotation reads at small
## scale. Built in code (no external assets, keeps the tscn lean).
func _build_wheels() -> void:
	var circle := PackedVector2Array()
	for i in 12:
		var t := float(i) / 12.0 * TAU
		circle.push_back(Vector2(cos(t), sin(t)) * WHEEL_RADIUS)
	var lug := PackedVector2Array([
		Vector2(-WHEEL_RADIUS, -0.02), Vector2(WHEEL_RADIUS, -0.02),
		Vector2(WHEEL_RADIUS, 0.02), Vector2(-WHEEL_RADIUS, 0.02),
	])
	for offset in WHEEL_OFFSETS:
		var wheel := Node2D.new()
		wheel.position = offset
		var disc := Polygon2D.new()
		disc.polygon = circle
		disc.color = WHEEL_COLOR
		var bar := Polygon2D.new()
		bar.polygon = lug
		bar.color = WHEEL_LUG_COLOR
		wheel.add_child(disc)
		wheel.add_child(bar)
		add_child(wheel)
		_wheels.push_back(wheel)


## A small warm light so the rover feels alive in darker corners.
## PointLight2D needs a texture - a programmatic radial falloff.
func _build_light() -> void:
	var light := PointLight2D.new()
	light.name = "RoverLight"
	light.texture = _radial_texture()
	light.texture_scale = LIGHT_SPAN
	light.energy = LIGHT_ENERGY
	light.color = LIGHT_COLOR
	light.shadow_enabled = false
	light.position = Vector2(0.1, 0.0)
	add_child(light)


static func _radial_texture() -> ImageTexture:
	var img := Image.create(64, 64, false, Image.FORMAT_RGBA8)
	for x in 64:
		for y in 64:
			var d := Vector2(x - 31.5, y - 31.5).length() / 31.5
			img.set_pixel(x, y, Color(1, 1, 1, clampf(1.0 - d, 0.0, 1.0) ** 2))
	return ImageTexture.create_from_image(img)
