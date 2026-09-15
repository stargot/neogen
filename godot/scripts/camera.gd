# Development camera (backlog 4.1).
#
# 2D by design: the simulation core is a flat Vec2 world and the rover
# mirror is a Node2D — the scene works in core units directly (coords has
# no scale factor); the on-screen size comes from the camera zoom.
#
# Controls (documented in README «Ручной тест 4.1»):
#   - pan: middle-button drag, or left-button drag while holding Space;
#   - zoom: mouse wheel, zoomed toward the cursor, clamped to
#     [MIN_ZOOM, MAX_ZOOM] (instant stepping — smoothing can come with
#     the art pass).
extends Camera2D

const MIN_ZOOM := 0.02   # far out: the whole pad and grid in view
const MAX_ZOOM := 0.5    # close up: single grid cells
const ZOOM_STEP := 1.25

var _panning := false


func _ready() -> void:
	zoom = Vector2(0.06, 0.06)  # start with the whole start pad framed
	position = Vector2.ZERO
	make_current()


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		_handle_button(event)
	elif event is InputEventMouseMotion and _panning:
		# Drag the world opposite to the mouse; compensate for zoom.
		position -= event.relative / zoom.x


func _handle_button(event: InputEventMouseButton) -> void:
	if not event.pressed:
		if event.button_index in [MOUSE_BUTTON_LEFT, MOUSE_BUTTON_MIDDLE]:
			_panning = false
		return
	match event.button_index:
		MOUSE_BUTTON_MIDDLE:
			_panning = true
		MOUSE_BUTTON_LEFT:
			if Input.is_key_pressed(KEY_SPACE):
				_panning = true
		MOUSE_BUTTON_WHEEL_UP:
			_zoom_toward_cursor(ZOOM_STEP)
		MOUSE_BUTTON_WHEEL_DOWN:
			_zoom_toward_cursor(1.0 / ZOOM_STEP)


func _zoom_toward_cursor(factor: float) -> void:
	var before := get_global_mouse_position()
	var next := clampf(zoom.x * factor, MIN_ZOOM, MAX_ZOOM)
	zoom = Vector2(next, next)
	# Keep the world point under the cursor anchored.
	position += before - get_global_mouse_position()
