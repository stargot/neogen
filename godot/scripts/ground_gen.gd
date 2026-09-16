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

const FNV_OFFSET := 0xcbf29ce484222325  # wraps negative - fine
const FNV_PRIME := 0x100000001b3

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


## Digest of the whole (2*half)x(2*half) layout around the origin - the
## determinism test compares these across runs. Pure.
static func layout_hash(seed: int, half: int) -> int:
	var h: int = FNV_OFFSET
	for y in range(-half, half):
		for x in range(-half, half):
			h = (h * FNV_PRIME) ^ cell_hash(seed, x, y)
	return h
