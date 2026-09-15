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

var _sim = null
var _rover_id: int = 1
var _trail: Line2D


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


func _process(_delta: float) -> void:
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

	_update_trail()


func _update_trail() -> void:
	var here: Vector2 = get_parent().global_position
	var points := _trail.points
	if points.size() > 0 and here.distance_to(points[points.size() - 1]) < TRAIL_MIN_STEP:
		return
	_trail.add_point(here)
	if points.size() + 1 > TRAIL_MAX_POINTS:
		_trail.remove_point(0)
