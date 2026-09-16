# Facade ground generation (backlog 6.5.1) — PURE static functions, no
# scene, no state: a deterministic cell layout from the CORE SEED only.
#
# This is a deliberately temporary duplicate of world flavor on the
# Godot side: tiles do not exist in neogen-core until phase 7, and the
# facade must not steal 7.1's design or touch the core. Everything here
# is scrapped when real tiles arrive.
#
# Hash: FNV-1a-flavoured mixing over 64-bit ints. GDScript ints are
# 64-bit SIGNED and arithmetic wraps silently modulo 2^64 - exactly the
# determinism the hash needs (verified by tests/facade_determinism.gd:
# same inputs -> same layout, across runs and across the wrap).
#
# Palette constants live here so the polish pass tweaks one place.
extends RefCounted

enum Kind { MOSS_LIGHT, MOSS_DARK, DIRT, STONE }
enum Prop { NONE, GRASS, ROCK, CRYSTAL }

const FNV_OFFSET := 0xcbf29ce484222325  # wraps negative - fine
const FNV_PRIME := 0x100000001b3

# --- Prop placement (6.5.3) -------------------------------------------
# Keep-outs (documented rules):
#   - the start pad: cells with |x|,|y| <= PAD_CLEAR stay prop-free
#     (PAD_CLEAR mirrors grid.gd's PAD_HALF_SIZE = core 8 + 1 margin);
#   - the patrol track: the patrol.lua route is the square through
#     (0,0),(8,0),(8,8),(0,8); a corridor of TRACK_CORRIDOR around
#     those four segments stays clear so the rover drives clean.
#     With the default pad both rules overlap on the pad area - the
#     track rule is kept explicit for when the pad constants change.
#   - the first approach leg (seeded start -> first corner) is a
#     straight core line that may cross props outside the pad: props
#     are flat visuals, accepted for the facade (documented limit).
const PAD_CLEAR := 9
const TRACK_HALF := 8
const TRACK_CORRIDOR := 0.8

# Prop frequencies (per cell, after keep-outs): grass is verdancy-driven
# (base in barren spots, lush in green patches); rocks and crystals are
# fixed-rare. Deliberately sparse - a hint of life, not a forest.
const GRASS_BASE := 0.05
const GRASS_LUSH := 0.32
const ROCK_FREQ := 0.015
const CRYSTAL_FREQ := 0.005
# Macro variation: verdancy is a low-frequency smoothed hash (patches of
# about VERDANCY_PATCH cells) driving the grass density.
const VERDANCY_PATCH := 8

# Cell palette (muted solarpank ground; the per-cell jitter lightens
# each cell by up to JITTER via the cell hash for an organic feel).
const COLORS := {
	Kind.MOSS_LIGHT: Color(0.42, 0.53, 0.35),
	Kind.MOSS_DARK: Color(0.31, 0.42, 0.28),
	Kind.DIRT: Color(0.52, 0.45, 0.33),
	Kind.STONE: Color(0.47, 0.50, 0.52),
}
const JITTER := 0.07

# Kind frequencies: light moss 45%, dark moss 30%, dirt 20%, stone 5%
# (rare) - uniform thanks to the avalanche finalizer. The pad itself is
# drawn separately by grid.gd on top.
const FREQ_LIGHT := 0.45
const FREQ_DARK := 0.75
const FREQ_DIRT := 0.95


## Deterministic cell hash from (seed, x, y). Pure.
static func cell_hash(seed: int, x: int, y: int) -> int:
	var h: int = seed
	h = (h ^ (x & 0xFFFFFFFF)) * FNV_PRIME
	h = (h ^ ((x >> 32) & 0xFFFFFFFF)) * FNV_PRIME
	h = (h ^ (y & 0xFFFFFFFF)) * FNV_PRIME
	h = (h ^ ((y >> 32) & 0xFFFFFFFF)) * FNV_PRIME
	return _avalanche(h)


## Murmur3-style finalizer: without it the raw FNV stream is skewed and
## kind frequencies drift far from their targets (measured 10-18% stone).
static func _avalanche(h: int) -> int:
	h ^= h >> 33
	h *= 0xff51afd7ed558ccd
	h ^= h >> 33
	h *= 0xc4ceb9fe1a85ec53
	h ^= h >> 33
	return h


## Ground kind of one world cell. Pure.
static func cell_kind(seed: int, x: int, y: int) -> int:
	var bits := (cell_hash(seed, x, y) >> 11) & 0x7FFFFFFF
	var frac := float(bits) / float(0x7FFFFFFF)
	if frac < FREQ_LIGHT:
		return Kind.MOSS_LIGHT
	if frac < FREQ_DARK:
		return Kind.MOSS_DARK
	if frac < FREQ_DIRT:
		return Kind.DIRT
	return Kind.STONE


## Render color of one cell (base kind color + deterministic jitter). Pure.
static func cell_color(seed: int, x: int, y: int) -> Color:
	var kind := cell_kind(seed, x, y)
	var bits := cell_hash(seed, x, y) & 0x3FF
	var jitter := float(bits) / 1023.0 * JITTER
	return COLORS[kind].lightened(jitter)


## Low-frequency "how green is this area" (0..1): bilinear-smoothed hash
## over a coarse patch grid - gives biome-like patches instead of uniform
## porridge. Pure.
static func verdancy(seed: int, x: int, y: int) -> float:
	var px := float(x) / VERDANCY_PATCH
	var py := float(y) / VERDANCY_PATCH
	var cx := floori(px)
	var cy := floori(py)
	var fx := px - cx
	var fy := py - cy
	var v00 := _corner(seed, cx, cy)
	var v10 := _corner(seed, cx + 1, cy)
	var v01 := _corner(seed, cx, cy + 1)
	var v11 := _corner(seed, cx + 1, cy + 1)
	var top := lerpf(v00, v10, fx)
	var bottom := lerpf(v01, v11, fx)
	return lerpf(top, bottom, fy)


static func _corner(seed: int, cx: int, cy: int) -> float:
	var bits := (cell_hash(seed ^ 0x9E37, cx * 7 + 13, cy * 11 + 5) >> 11) & 0x7FFFFFFF
	return float(bits) / float(0x7FFFFFFF)


## Keep-out check: the start pad and the patrol track corridor. Pure.
static func is_keepout(x: int, y: int) -> bool:
	if absi(x) <= PAD_CLEAR and absi(y) <= PAD_CLEAR:
		return true
	var cx := x + 0.5
	var cy := y + 0.5
	return (
		_dist_to_segment(cx, cy, 0.0, 0.0, TRACK_HALF, 0.0) <= TRACK_CORRIDOR
		or _dist_to_segment(cx, cy, TRACK_HALF, 0.0, TRACK_HALF, TRACK_HALF) <= TRACK_CORRIDOR
		or _dist_to_segment(cx, cy, TRACK_HALF, TRACK_HALF, 0.0, TRACK_HALF) <= TRACK_CORRIDOR
		or _dist_to_segment(cx, cy, 0.0, TRACK_HALF, 0.0, 0.0) <= TRACK_CORRIDOR
	)


static func _dist_to_segment(px: float, py: float, ax: float, ay: float, bx: float, by: float) -> float:
	var abx := bx - ax
	var aby := by - ay
	var t := clampf(((px - ax) * abx + (py - ay) * aby) / (abx * abx + aby * aby), 0.0, 1.0)
	var dx := px - (ax + t * abx)
	var dy := py - (ay + t * aby)
	return sqrt(dx * dx + dy * dy)


## Prop kind of one cell (NONE on keep-outs). Pure.
static func prop(seed: int, x: int, y: int) -> int:
	if is_keepout(x, y):
		return Prop.NONE
	var h := cell_hash(seed, x, y)
	# Independent hash slices per decision.
	var crystal := float((h >> 3) & 0xFFF) / 4095.0
	if crystal < CRYSTAL_FREQ:
		return Prop.CRYSTAL
	var rock := float((h >> 17) & 0xFFF) / 4095.0
	if rock < ROCK_FREQ:
		return Prop.ROCK
	var grass := float((h >> 31) & 0xFFF) / 4095.0
	var p := GRASS_BASE + verdancy(seed, x, y) * (GRASS_LUSH - GRASS_BASE)
	if grass < p:
		return Prop.GRASS
	return Prop.NONE


## Per-prop render parameters: rotation, size scale, in-cell offset,
## variant bits. Pure.
static func prop_params(seed: int, x: int, y: int) -> Dictionary:
	var h := cell_hash(seed ^ 0xC0FFEE, x, y)
	return {
		"rot": float(h & 0x3FF) / 1024.0 * TAU,
		"scale": 0.8 + float((h >> 10) & 0x3FF) / 1024.0 * 0.5,
		"ox": float((h >> 20) & 0xFF) / 255.0 * 0.6 - 0.3,
		"oy": float((h >> 28) & 0xFF) / 255.0 * 0.6 - 0.3,
		"variant": (h >> 38) & 0x3,
	}


## Digest of the prop layout (kinds folded with the parameter hashes) -
## the determinism test compares these across runs. Pure.
static func prop_layout_hash(seed: int, half: int) -> int:
	var h: int = FNV_OFFSET
	for y in range(-half, half):
		for x in range(-half, half):
			var p := prop(seed, x, y)
			var params := prop_params(seed, x, y)
			var fold: int = cell_hash(seed, x, y) + p * 7919
			fold += int(params.rot * 1024.0) + int(params.scale * 512.0)
			fold += int(params.ox * 256.0) * 31 + int(params.oy * 256.0) * 17
			h = (h * FNV_PRIME) ^ fold
	return h


## Digest of the whole (2*half)x(2*half) layout around the origin - the
## determinism test compares these across runs. Pure.
static func layout_hash(seed: int, half: int) -> int:
	var h: int = FNV_OFFSET
	for y in range(-half, half):
		for x in range(-half, half):
			h = (h * FNV_PRIME) ^ cell_hash(seed, x, y)
	return h
