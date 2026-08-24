${{ activate_function }}() {
  local output
  trap '' SIGINT
  output=$(${{ activate_command }})
  if [ -n "$output" ]; then
    eval "$output";
  fi
  trap - SIGINT
}

${{ deactivate_function }}() {
  local output
  output=$(${{ deactivate_command }})
  if [ -n "$output" ]; then
    eval "$output";
  fi
  chpwd_functions=(${chpwd_functions:#${{ activate_function }}})
  precmd_functions=(${precmd_functions:#${{ activate_function }}})
  unfunction ${{ activate_function }} ${{ deactivate_function }} 2>/dev/null
}

typeset -ag chpwd_functions
if (( ! ${chpwd_functions[(I)${{ activate_function }}]} )); then
  chpwd_functions=(${{ activate_function }} $chpwd_functions)
fi

typeset -ag precmd_functions
if (( ! ${precmd_functions[(I)${{ activate_function }}]} )); then
  precmd_functions=(${{ activate_function }} $precmd_functions)
fi
