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

	# 5. Stone stays rare (<= 12% of cells for seed 42).
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
