# An event handler is part of a function definition, so the trigger calls the
# function rather than being attached to it
function ${{ function }}_on_context --on-variable PWD --on-event fish_prompt;
  ${{ function }}
end;
