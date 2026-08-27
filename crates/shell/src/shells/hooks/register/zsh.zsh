typeset -ag chpwd_functions
if (( ! ${chpwd_functions[(I)${{ function }}]} )); then
  chpwd_functions=(${{ function }} $chpwd_functions)
fi

typeset -ag precmd_functions
if (( ! ${precmd_functions[(I)${{ function }}]} )); then
  precmd_functions=(${{ function }} $precmd_functions)
fi
