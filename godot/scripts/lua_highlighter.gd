# Lua syntax highlighting for the script editor (backlog 5.2).
#
# Warm, restrained solarpunk-dev palette - readability first:
#   - keywords (control flow and value words): amber      #d99e4d
#   - strings ('...' / "..."):              soft green   #9ec78c
#   - comments (-- and --[[...]]):           grey-olive   #8c8c7a
#   - numbers:                               soft violet  #b8a6d9
#   - function keyword & calls:              warm ivory   #e6e1bf
extends CodeHighlighter

const KEYWORD_COLOR := Color("d99e4d")
const STRING_COLOR := Color("9ec78c")
const COMMENT_COLOR := Color("8c8c7a")
const NUMBER_COLOR := Color("b8a6d9")
const FUNCTION_COLOR := Color("e6e1bf")

const KEYWORDS := [
	"and", "break", "do", "else", "elseif", "end", "false", "for",
	"function", "goto", "if", "in", "local", "nil", "not", "or",
	"repeat", "return", "then", "true", "until", "while",
]


func _init() -> void:
	number_color = NUMBER_COLOR
	function_color = FUNCTION_COLOR
	for word in KEYWORDS:
		add_keyword_color(word, KEYWORD_COLOR)
	# Strings: both quote kinds, single-line.
	add_color_region("'", "'", STRING_COLOR, true)
	add_color_region("\"", "\"", STRING_COLOR, true)
	# Comments: line (--) and block (--[[ ]]).
	# MVP limitation (review #10): the `--` region is checked first and
	# paints the `--[[` opener as a line comment, so block comments render
	# line-by-line in the same color - accepted for now, revisit with the
	# art pass.
	add_color_region("--", "", COMMENT_COLOR, true)
	add_color_region("--[[", "]]", COMMENT_COLOR, false)
