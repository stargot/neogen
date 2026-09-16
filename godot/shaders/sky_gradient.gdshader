shader_type canvas_item;

// Neogen sky (backlog 6.5.1): static warm-dawn gradient -
// amber horizon into a muted teal zenith. No animation yet.

uniform vec3 horizon_color : source_color = vec3(0.93, 0.68, 0.42);
uniform vec3 zenith_color : source_color = vec3(0.38, 0.55, 0.55);
uniform float horizon_falloff : hint_range(0.5, 4.0) = 1.5;

void fragment() {
	// UV.y: 0 at the top of the screen (zenith), 1 at the bottom (horizon).
	float t = clamp(UV.y, 0.0, 1.0);
	vec3 sky = mix(zenith_color, horizon_color, pow(t, horizon_falloff));
	COLOR = vec4(sky, 1.0);
}
