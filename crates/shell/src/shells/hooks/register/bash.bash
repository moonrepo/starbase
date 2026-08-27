if [[ "$(declare -p PROMPT_COMMAND 2>&1)" == "declare -a"* ]]; then
  if [[ " ${PROMPT_COMMAND[*]:-} " != *" ${{ function }} "* ]]; then
    PROMPT_COMMAND=(${{ function }} "${PROMPT_COMMAND[@]}")
  fi
elif [[ ";${PROMPT_COMMAND:-};" != *";${{ function }};"* ]]; then
  PROMPT_COMMAND="${{ function }}${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
