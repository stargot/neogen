shader_type canvas_item;

// Neogen vignette (backlog 6.5.2): a light warm-dark corner falloff.
// Deliberately gentle - it must frame the picture, not fight the HUD
// and console (both draw above this layer).

uniform float strength : hint_range(0.0, 1.0) = 0.32;
uniform float start : hint_range(0.2, 1.2) = 0.62;
uniform vec3 tint : source_color = vec3(0.10, 0.08, 0.06);

void fragment() {
	// Distance from screen center, normalized so a corner reads ~1.0.
	vec2 uv = UV - vec2(0.5);
	float d = length(uv) * 1.4142;
	float v = smoothstep(start, 1.0, d) * strength;
	COLOR = vec4(tint, v);
}
