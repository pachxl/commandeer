#!/bin/sh
# @raycast.schemaVersion 1
# @raycast.title Open Scripts Folder
# @raycast.description Open the folder containing your Commandeer scripts
# @raycast.icon folder
# @raycast.mode silent
# @vicinae.keywords ["commands", "examples", "files"]

scripts_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
case "$(uname -s)" in
  Darwin) open "$scripts_dir" ;;
  Linux) xdg-open "$scripts_dir" ;;
  *) echo "Unsupported platform" >&2; exit 1 ;;
esac
