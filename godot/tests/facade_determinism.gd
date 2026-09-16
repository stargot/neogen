# Facade determinism test (backlog 6.5.1) — no scene, pure ground_gen:
# same seed -> identical layout hash (across runs and across 64-bit
# wrap-around), different seed -> different layout, and the layout uses
# all four ground kinds.
#
# Run from godot/:
#   godot --headless --script res://tests/facade_determinism.gd
extends SceneTree

var _failures := 0


func _initialize() -> void:
	var gg = load("res://scripts/ground_gen.gd")

	# 1. Determinism: the same seed hashes identically (this is also the
	#    ASSUMPTION check: GDScript ints are 64-bit and the FNV-style
	#    multiplications overflow silently - the wrap must be stable).
	var a: int = gg.layout_hash(42, 16)
	var b: int = gg.layout_hash(42, 16)
	_check(a == b, "seed 42 -> identical layout hash on both runs")
	var c: int = gg.layout_hash(43, 16)
	_check(a != c, "seed 43 -> different layout hash")

	# 2. Wrap-around behavior is exactly modulo 2^64 (silent, stable).
	_check((1 << 63) * 2 == 0, "int wrap: 2^63 * 2 == 0 (mod 2^64)")
	_check(0x7FFFFFFFFFFFFFFF + 1 == -0x8000000000000000, "int wrap: max + 1 == min")

	# 3. Per-cell purity: kinds and colors are stable per coordinate.
	var k1: int = gg.cell_kind(42, 7, -3)
	_check(k1 == gg.cell_kind(42, 7, -3), "cell_kind is pure")
	_check(
		gg.cell_color(42, 7, -3) == gg.cell_color(42, 7, -3),
		"cell_color is pure"
	)

	# 4. The layout around the start uses all four kinds (64x64, seed 42).
	var kinds := {}
	for y in range(-32, 32):
		for x in range(-32, 32):
			kinds[gg.cell_kind(42, x, y)] = true
	_check(kinds.size() == 4, "all four ground kinds present, got %d" % kinds.size())

	# 5. Props (6.5.3): deterministic, seed-divergent, kind-complete.
	var pa: int = gg.prop_layout_hash(42, 16)
	var pb: int = gg.prop_layout_hash(42, 16)
	_check(pa == pb, "seed 42 -> identical prop layout hash on both runs")
	var pc: int = gg.prop_layout_hash(43, 16)
	_check(pa != pc, "seed 43 -> different prop layout hash")
	_check(
		gg.prop(42, 7, -3) == gg.prop(42, 7, -3) and gg.prop_params(42, 7, -3) == gg.prop_params(42, 7, -3),
		"prop and prop_params are pure"
	)

	# Keep-outs: the pad and the patrol track stay prop-free.
	_check(gg.prop(42, 0, 0) == gg.Prop.NONE, "pad center has no props")
	_check(gg.prop(42, 7, 0) == gg.Prop.NONE, "patrol track row stays clear")
	var pad_violations := 0
	for y in range(-9, 10):
		for x in range(-9, 10):
			if gg.prop(42, x, y) != gg.Prop.NONE:
				pad_violations += 1
	_check(pad_violations == 0, "no props anywhere on the start pad")

	# All three prop kinds occur on the 64x64 window (seed 42).
	var prop_kinds := {}
	var grass := 0
	var rocks := 0
	var crystals := 0
	for y in range(-32, 32):
		for x in range(-32, 32):
			var kind: int = gg.prop(42, x, y)
			if kind != gg.Prop.NONE:
				prop_kinds[kind] = true
			if kind == gg.Prop.GRASS:
				grass += 1
			elif kind == gg.Prop.ROCK:
				rocks += 1
			elif kind == gg.Prop.CRYSTAL:
				crystals += 1
	_check(prop_kinds.size() == 3, "grass, rock and crystal all present")
	_check(grass <= 1500, "grass stays sparse-ish: %d/4096" % grass)
	_check(rocks <= 200, "rocks rare: %d/4096" % rocks)
	_check(crystals <= 100, "crystals rare: %d/4096" % crystals)

	# Verdancy (macro variation) is pure and in range.
	var v42: float = gg.verdancy(42, 3, 3)
	_check(v42 == gg.verdancy(42, 3, 3), "verdancy is pure")
	_check(v42 >= 0.0 and v42 <= 1.0, "verdancy in 0..1")

	# 6. Settlement (6.5.4): deterministic, seed-divergent, keep-outs hold.
	var sg = load("res://scripts/settlement.gd")
	var sa: Array = sg.layout(42)
	var sb: Array = sg.layout(42)
	_check(sa == sb, "seed 42 -> identical settlement layout on both runs")
	_check(sg.layout_hash(42) == sg.layout_hash(42), "settlement layout hash is stable")
	_check(sg.layout_hash(42) != sg.layout_hash(43), "seed 43 -> different settlement")
	_check(sa.size() >= 4 and sa.size() <= 5, "cluster has 4-5 structures, got %d" % sa.size())
	var struct_kinds := {}
	for st in sa:
		struct_kinds[st.kind] = true
	_check(
		struct_kinds.has(sg.Kind.DOME) and struct_kinds.has(sg.Kind.PANEL) and struct_kinds.has(sg.Kind.MAST),
		"cluster composition: dome + panels + mast"
	)
	var keepout_violations := 0
	var on_blocking := 0
	for st in sa:
		# Outside the start pad (with the prop keep-out margin).
		if absf(st.pos.x) <= 10.0 and absf(st.pos.y) <= 10.0:
			keepout_violations += 1
		# Off the patrol track corridor.
		if absf(st.pos.y) <= 1.6 and st.pos.x >= -1.0 and st.pos.x <= 9.0:
			keepout_violations += 1
		# Not on a rock/crystal cell.
		if sg._on_blocking_prop(42, st.pos):
			on_blocking += 1
	_check(keepout_violations == 0, "settlement stays off the pad and track")
	_check(on_blocking == 0, "settlement avoids rock/crystal cells")

	# 7. Stone stays rare (<= 12% of cells for seed 42).
	var stones := 0
	for y in range(-32, 32):
		for x in range(-32, 32):
			if gg.cell_kind(42, x, y) == gg.Kind.STONE:
				stones += 1
	_check(
		stones <= 245,
		"stone stays rare: %d/4096 cells (target 5%%)" % stones
	)

	_finish()


func _check(condition: bool, message: String) -> void:
	if condition:
		print("ok   - ", message)
	else:
		_failures += 1
		push_error("FAIL - " + message)


func _finish() -> void:
	if _failures == 0:
		print("FACADE DETERMINISM OK")
		quit(0)
	else:
		print("FACADE DETERMINISM FAILED: %d check(s)" % _failures)
		quit(1)
