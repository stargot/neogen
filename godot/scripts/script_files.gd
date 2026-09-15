# Script file access for the editor panel (backlog 5.1).
#
# DEV-BUILD PATH LIMITATION (documented decision): the player scripts
# folder lives in the REPOSITORY ROOT (godot/../scripts), not in res://.
# We resolve it via ProjectSettings.globalize_path("res://") and work
# with absolute paths - writable in editor/dev builds. In an exported
# game res:// is read-only packed; the export phase (post-MVP) must
# switch this to a user:// workspace with import of bundled scripts.
#
# MVP write model: FULL REWRITE of the file on save (no diffs).
#
# Safety: file names are validated against a strict whitelist pattern -
# no separators, no "..", no absolute paths (the editor must never escape
# the scripts folder).
extends RefCounted

const MAX_FILE_BYTES := 256 * 1024

# Strict whitelist: alnum/underscore head, then alnum/_/./-, ".lua" tail -
# no separators, no leading dot, so no traversal out of the folder.
static var _NAME_RE := RegEx.create_from_string("^[A-Za-z0-9_][A-Za-z0-9_.-]*\\.lua$")


## Absolute path of the scripts folder (see the header note).
static func dir_path() -> String:
	return ProjectSettings.globalize_path("res://").path_join("..").path_join("scripts").simplify_path()


## Whether a candidate file name is safe and well-formed.
static func valid_name(file_name: String) -> bool:
	return _NAME_RE.search(file_name) != null


## All `.lua` file names in the folder, sorted; missing folder -> empty.
func scan() -> Array[String]:
	var out: Array[String] = []
	var dir := DirAccess.open(dir_path())
	if dir == null:
		push_warning("script_files: cannot open %s" % dir_path())
		return out
	dir.list_dir_begin()
	var entry := dir.get_next()
	while not entry.is_empty():
		if not dir.current_is_dir() and entry.ends_with(".lua"):
			out.append(entry)
		entry = dir.get_next()
	dir.list_dir_end()
	out.sort()
	return out


## Read a file's text; "" when unreadable (error pushed).
func read(file_name: String) -> String:
	if not valid_name(file_name):
		push_error("script_files: refusing to read %s" % file_name)
		return ""
	var fa := FileAccess.open(_path(file_name), FileAccess.READ)
	if fa == null:
		push_error("script_files: cannot read %s" % file_name)
		return ""
	return fa.get_as_text()


## Write a file (FULL REWRITE, MVP). Returns OK or an error code.
func write(file_name: String, text: String) -> int:
	if not valid_name(file_name):
		push_error("script_files: refusing to write %s" % file_name)
		return ERR_INVALID_PARAMETER
	if text.to_utf8_buffer().size() > MAX_FILE_BYTES:
		return ERR_INVALID_DATA
	var dir := DirAccess.open(dir_path())
	if dir == null:
		return ERR_CANT_OPEN
	var fa := FileAccess.open(_path(file_name), FileAccess.WRITE)
	if fa == null:
		return FileAccess.get_open_error()
	fa.store_string(text)
	return OK


## Delete a file (tests cleanup).
func remove(file_name: String) -> int:
	if not valid_name(file_name):
		return ERR_INVALID_PARAMETER
	return DirAccess.remove_absolute(_path(file_name))


## First free name of the form prefix_1.lua, prefix_2.lua, ...
func unique_name(prefix: String) -> String:
	var existing := scan()
	var n := 1
	var candidate := "%s_%d.lua" % [prefix, n]
	while existing.has(candidate):
		n += 1
		candidate = "%s_%d.lua" % [prefix, n]
	return candidate


func _path(file_name: String) -> String:
	return dir_path().path_join(file_name)
