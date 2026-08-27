if [[ "$(declare -p PROMPT_COMMAND 2>&1)" == "declare -a"* ]]; then
  ${{ function }}_filtered=();
  for ${{ function }}_entry in "${PROMPT_COMMAND[@]}"; do
    if [[ "$${{ function }}_entry" != "${{ function }}" ]]; then
      ${{ function }}_filtered+=("$${{ function }}_entry");
    fi
  done
  PROMPT_COMMAND=("${${{ function }}_filtered[@]}");
  unset ${{ function }}_filtered ${{ function }}_entry;
else
  PROMPT_COMMAND=";${PROMPT_COMMAND:-};";
  PROMPT_COMMAND="${PROMPT_COMMAND//;${{ function }};/;}";
  PROMPT_COMMAND="${PROMPT_COMMAND#;}";
  PROMPT_COMMAND="${PROMPT_COMMAND%;}";
fi
