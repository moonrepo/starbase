fn ${{ function }} {|@_|
  eval (${{ command }} | slurp)
}

# Consumers `eval` this code, which runs in a namespace that is thrown away, so
# the function is exported for the session to be able to call it
try {
  edit:add-vars [&'${{ function }}~'=$${{ function }}~]
} catch _ {
  # The `edit:` module only exists in an interactive session
}
