# The filter is inlined rather than pulled into a function, so that this can be
# evaluated in the same namespace as the registration without redefining one
set @after-chdir = (each {|handler| if (not (and (has-key $handler def) (eq $handler[def] $${{ function }}~[def]))) { put $handler } } $after-chdir)

# The `edit:` module only exists in an interactive session, and referencing it
# directly is a compilation error, which `try` cannot catch
try {
  eval &ns=(ns [&activate~=$${{ function }}~]) 'set @edit:before-readline = (each {|handler| if (not (and (has-key $handler def) (eq $handler[def] $activate~[def]))) { put $handler } } $edit:before-readline)'
} catch _ {
  # Nothing was registered when non-interactive
}
