# Re-sourcing creates a new function object every time, so a registered handler
# is matched by its body rather than by identity
fn ${{ function }}_on_chdir {|handlers|
  each {|handler| if (not (and (has-key $handler def) (eq $handler[def] $${{ function }}~[def]))) { put $handler } } $handlers
}

set @after-chdir = (${{ function }}_on_chdir $after-chdir) $${{ function }}~

# The prompt trigger lives in the `edit:` module, which only exists in an
# interactive session. Referencing it directly is a compilation error, which
# `try` cannot catch, so the registration is compiled at runtime by `eval`
try {
  eval &ns=(ns [&others~=$${{ function }}_on_chdir~ &activate~=$${{ function }}~]) 'set @edit:before-readline = (others $edit:before-readline) $activate~'
} catch _ {
  # The `edit:` module only exists in an interactive session
}
