fn ${{ activate_function }} {|@_|
  eval (${{ activate_command }} | slurp)
}

fn ${{ deactivate_function }} {
  eval (${{ deactivate_command }} | slurp)
  set after-chdir = [(each {|hook| if (not (and (has-key $hook def) (eq $hook[def] $${{ activate_function }}~[def]))) { put $hook } } $after-chdir)]
  try {
    eval &ns=(ns [&activate~=$${{ activate_function }}~]) 'set edit:before-readline = [(each {|hook| if (not (and (has-key $hook def) (eq $hook[def] $activate~[def]))) { put $hook } } $edit:before-readline)]'
    edit:del-vars ['${{ activate_function }}~' '${{ deactivate_function }}~']
  } catch _ {
    # Nothing was registered or exported when non-interactive
  }
}

set after-chdir = [(each {|hook| if (not (and (has-key $hook def) (eq $hook[def] $${{ activate_function }}~[def]))) { put $hook } } $after-chdir)]
set @after-chdir = $@after-chdir $${{ activate_function }}~

# The prompt trigger lives in the `edit:` module, which only exists in an
# interactive session. Referencing it directly is a compilation error, which
# `try` cannot catch, so the registration is compiled at runtime by `eval`
try {
  eval &ns=(ns [&activate~=$${{ activate_function }}~]) 'set edit:before-readline = [(each {|hook| if (not (and (has-key $hook def) (eq $hook[def] $activate~[def]))) { put $hook } } $edit:before-readline)]
set @edit:before-readline = $@edit:before-readline $activate~'
} catch _ {
  # The `edit:` module only exists in an interactive session
}

# Consumers `eval` this code, which runs in a restricted namespace, so export
# both functions for the interactive session to be able to call them
try {
  edit:add-vars [&'${{ activate_function }}~'=$${{ activate_function }}~ &'${{ deactivate_function }}~'=$${{ deactivate_function }}~]
} catch _ {
  # The `edit:` module only exists in an interactive session
}
