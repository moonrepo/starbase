${{ function }}() {
  local previous_exit_status=$?;
  local output;
  trap '' SIGINT;
  output=$(${{ command }})
  if [ -n "$output" ]; then
    eval "$output";
  fi
  trap - SIGINT;
  return $previous_exit_status;
};
