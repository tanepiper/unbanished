# Sourced by the compat scripts. run_name BINARY prints the name of BINARY's results folder: its
# file name and a short hash of its full path. Two sessions that test binaries with the same file
# name from different folders then keep their results apart. With only the file name, one run
# deleted and overwrote the other's saves.
run_name() {
  local full
  full="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
  printf '%s-%s' "$(basename "$1")" "$(printf '%s' "$full" | shasum | cut -c1-8)"
}
