fn ${{ activate_function }} {|@_|
  eval (${{ activate_command }} | slurp)
}

# Re-sourcing creates a new function object every time, so a registered handler
# is matched by its body rather than by identity
fn ${{ activate_function }}_others {|handlers|
  each {|handler| if (not (and (has-key $handler def) (eq $handler[def] $${{ activate_function }}~[def]))) { put $handler } } $handlers
}

fn ${{ deactivate_function }} {
  eval (${{ deactivate_command }} | slurp)
  set @after-chdir = (${{ activate_function }}_others $after-chdir)
  try {
    eval &ns=(ns [&others~=$${{ activate_function }}_others~]) 'set @edit:before-readline = (others $edit:before-readline)'
    edit:del-vars ['${{ activate_function }}~' '${{ deactivate_function }}~']
  } catch _ {
    # Nothing was registered or exported when non-interactive
  }
}

set @after-chdir = (${{ activate_function }}_others $after-chdir) $${{ activate_function }}~

# The prompt trigger and the exported names both live in the `edit:` module,
# which only exists in an interactive session. Referencing it directly is a
# compilation error, which `try` cannot catch, so the registration is compiled
# at runtime by `eval`
try {
  eval &ns=(ns [&others~=$${{ activate_function }}_others~ &activate~=$${{ activate_function }}~]) 'set @edit:before-readline = (others $edit:before-readline) $activate~'
  edit:add-vars [&'${{ activate_function }}~'=$${{ activate_function }}~ &'${{ deactivate_function }}~'=$${{ deactivate_function }}~]
} catch _ {
  # The `edit:` module only exists in an interactive session
}
