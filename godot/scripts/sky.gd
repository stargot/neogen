# Sky layer (backlog 6.5.1): a fullscreen gradient quad on a CanvasLayer
# below the world. CanvasLayer content is screen-space, so the sky needs
# no viewport following - it simply sits under everything world-space.
extends CanvasLayer

const SKY_SHADER := preload("res://shaders/sky_gradient.gdshader")


func _ready() -> void:
	layer = -10  # below the world layer (0)
	var rect := ColorRect.new()
	rect.name = "Gradient"
	rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	rect.set_anchors_preset(Control.PRESET_FULL_RECT)
	var material := ShaderMaterial.new()
	material.shader = SKY_SHADER
	rect.material = material
	add_child(rect)
