${{ function }}() {
  ${{ function }}_output=$(${{ command }})
  if [ -n "$${{ function }}_output" ]; then
    eval "$${{ function }}_output"
  fi
  unset ${{ function }}_output
}
