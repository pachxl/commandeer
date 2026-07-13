# @raycast.schemaVersion 1
# @raycast.title Script Tutorial
# @raycast.description Open this file to learn how to add your own commands
# @raycast.icon note
# @raycast.mode inline
# @vicinae.refreshTime 1h
# @vicinae.keywords ["tutorial", "help", "scripts", "example", "docs"]
#
# ---------------------------------------------------------------------------
#  Commandeer script commands
# ---------------------------------------------------------------------------
#  Drop a .ps1 script in this folder and it appears in the palette. The header
#  comments above configure how it shows up. Everything is optional -- with no
#  directives, the file name becomes the title.
#
#  Supported directives. Each is written with an '@' prefix, like the header
#  above (they're listed without it here so this list isn't parsed as real
#  directives). All work as either raycast.* or vicinae.*:
#
#    raycast.title            Name shown in the palette
#    raycast.description      Subtitle / detail text
#    raycast.icon             A named icon: terminal, folder, note, clock, ...
#    raycast.mode             inline | silent | fullOutput  (badge in the row)
#    vicinae.refreshTime      For inline mode: re-run every 5s / 2m / 1h and
#                             show the latest stdout live in the row
#    vicinae.needsConfirmation true   Ask before running (destructive actions)
#    vicinae.keywords         JSON array of extra search terms
#    raycast.argument1        JSON like {"type":"text","placeholder":"name"} --
#                             up to argument3, prompts for input before running
#
#  This script runs in "inline" mode, so the line it prints below is shown as
#  its subtitle. Edit it, copy it, or delete it once you are comfortable --
#  Commandeer re-scans this folder every time the palette opens.
# ---------------------------------------------------------------------------

Write-Output "Edit tutorial.ps1 in your scripts folder to build your own commands"
