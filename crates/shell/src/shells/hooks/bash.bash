${{ activate_function }}() {
  local previous_exit_status=$?;
  local output;
  trap '' SIGINT;
  output=$(${{ activate_command }})
  if [ -n "$output" ]; then
    eval "$output";
  fi
  trap - SIGINT;
  return $previous_exit_status;
};

${{ deactivate_function }}() {
  local output;
  output=$(${{ deactivate_command }})
  if [ -n "$output" ]; then
    eval "$output";
  fi
  if [[ "$(declare -p PROMPT_COMMAND 2>&1)" == "declare -a"* ]]; then
    local filtered=();
    local entry;
    for entry in "${PROMPT_COMMAND[@]}"; do
      if [[ "$entry" != "${{ activate_function }}" ]]; then
        filtered+=("$entry");
      fi
    done
    PROMPT_COMMAND=("${filtered[@]}");
  else
    PROMPT_COMMAND=";${PROMPT_COMMAND:-};";
    PROMPT_COMMAND="${PROMPT_COMMAND//;${{ activate_function }};/;}";
    PROMPT_COMMAND="${PROMPT_COMMAND#;}";
    PROMPT_COMMAND="${PROMPT_COMMAND%;}";
  fi
  unset -f ${{ activate_function }} ${{ deactivate_function }};
};

if [[ "$(declare -p PROMPT_COMMAND 2>&1)" == "declare -a"* ]]; then
  if [[ " ${PROMPT_COMMAND[*]:-} " != *" ${{ activate_function }} "* ]]; then
    PROMPT_COMMAND=(${{ activate_function }} "${PROMPT_COMMAND[@]}")
  fi
elif [[ ";${PROMPT_COMMAND:-};" != *";${{ activate_function }};"* ]]; then
  PROMPT_COMMAND="${{ activate_function }}${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
