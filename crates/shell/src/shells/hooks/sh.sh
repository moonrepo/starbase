${{ activate_function }}() {
  ${{ activate_function }}_output=$(${{ activate_command }})
  if [ -n "$${{ activate_function }}_output" ]; then
    eval "$${{ activate_function }}_output"
  fi
  unset ${{ activate_function }}_output
}

${{ deactivate_function }}() {
  ${{ deactivate_function }}_output=$(${{ deactivate_command }})
  if [ -n "$${{ deactivate_function }}_output" ]; then
    eval "$${{ deactivate_function }}_output"
  fi
  unset ${{ deactivate_function }}_output
  unset -f cd ${{ activate_function }} ${{ deactivate_function }}
}

cd() {
  command cd "$@" || return $?
  ${{ activate_function }}
  return 0
}
