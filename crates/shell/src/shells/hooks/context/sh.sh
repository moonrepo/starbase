# POSIX shells have no change-dir or prompt hook, so `cd` is shadowed. This
# clobbers any other `cd` wrapper, as there is no way to chain functions
cd() {
  command cd "$@" || return $?
  ${{ function }}
  return 0
}
