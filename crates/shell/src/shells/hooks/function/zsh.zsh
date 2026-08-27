${{ function }}() {
  local output
  trap '' SIGINT
  output=$(${{ command }})
  if [ -n "$output" ]; then
    eval "$output";
  fi
  trap - SIGINT
}
