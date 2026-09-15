# Onboarding hint (backlog 6.1): the "where do I start" panel.
#
# MVP: shown at every start, closed by the button or Esc. A first-run
# flag may come post-MVP. Styled like the HUD (semi-transparent panel,
# default font); sits above everything else but does not block input
# under it (mouse_filter = ignore on the layer contents except the
# close button).
extends CanvasLayer

@onready var close_button: Button = $Panel/Margin/VBox/Close


func _ready() -> void:
	close_button.pressed.connect(close_hint)


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and event.keycode == KEY_ESCAPE:
		close_hint()
		get_viewport().set_input_as_handled()


func close_hint() -> void:
	visible = false
