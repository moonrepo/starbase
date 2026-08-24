function ${{ activate_function }} --on-variable PWD --on-event fish_prompt;
  ${{ activate_command }} | source
end;

function ${{ deactivate_function }};
  ${{ deactivate_command }} | source
  functions --erase ${{ activate_function }} ${{ deactivate_function }}
end;
