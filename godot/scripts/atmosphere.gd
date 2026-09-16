# Atmosphere (backlog 6.5.2): rare warm pollen motes drifting on the
# wind, in WORLD coordinates around the camera.
#
# Particles choice (documented decision): CPUParticles2D, not
# GPUParticles2D - CPU simulation has no GPU dependency, runs
# identically in headless CI (boot_check / scene_integration) and on
# weak machines; ~36 particles cost nothing. A later art pass may
# migrate to GPUParticles2D for fancier effects.
#
# The emitter follows the camera each frame and resizes to the visible
# world area (viewport / zoom), so motes are always around the view -
# not left behind when the player pans. Extents are clamped so the
# far zoom-out does not explode the emission rectangle.
extends Node2D

const MOTE_COUNT := 36
const DRIFT_DIRECTION := Vector2(0.55, -1.0)  # up the wind, slightly east
const DRIFT_SPEED_MIN := 0.05                  # world units per second
const DRIFT_SPEED_MAX := 0.16
const LIFETIME := 9.0
const PREPROCESS := 4.0          # warm start: field is alive at boot
const MIN_EXTENT := 18.0         # clamp for close zoom
const MAX_EXTENT := 320.0        # clamp for far zoom

@onready var pollen: CPUParticles2D = $Pollen


func _ready() -> void:
	pollen.amount = MOTE_COUNT
	pollen.lifetime = LIFETIME
	pollen.preprocess = PREPROCESS
	pollen.emission_shape = CPUParticles2D.EMISSION_SHAPE_RECTANGLE
	pollen.direction = DRIFT_DIRECTION
	pollen.spread = 30.0
	pollen.gravity = Vector2.ZERO
	pollen.initial_velocity_min = DRIFT_SPEED_MIN
	pollen.initial_velocity_max = DRIFT_SPEED_MAX
	pollen.scale_amount_min = 0.035
	pollen.scale_amount_max = 0.09
	pollen.color = Color(1.0, 0.86, 0.55, 0.55)
	pollen.texture = _dot_texture()
	_fit_to_view()


func _process(_delta: float) -> void:
	_fit_to_view()


func _fit_to_view() -> void:
	var viewport := get_viewport()
	var camera := viewport.get_camera_2d()
	if camera != null:
		global_position = camera.get_screen_center_position()
	var zoom: float = maxf(camera.zoom.x if camera != null else 16.0, 0.001)
	var half := viewport.get_visible_rect().size * 0.5 / zoom
	pollen.emission_rect_extents = Vector2(
		clampf(half.x, MIN_EXTENT, MAX_EXTENT),
		clampf(half.y, MIN_EXTENT, MAX_EXTENT)
	)


## Programmatic soft dot (no external assets rule): 8x8 radial falloff.
static func _dot_texture() -> ImageTexture:
	var img := Image.create(8, 8, false, Image.FORMAT_RGBA8)
	for x in 8:
		for y in 8:
			var d := Vector2(x - 3.5, y - 3.5).length() / 3.5
			img.set_pixel(x, y, Color(1.0, 1.0, 1.0, clampf(1.0 - d, 0.0, 1.0)))
	return ImageTexture.create_from_image(img)
