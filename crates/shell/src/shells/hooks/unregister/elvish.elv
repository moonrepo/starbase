use str

# Handlers are matched by the name comment their definition carries, rather
# than by the function itself, which is out of reach here: this may be
# evaluated inside the deactivate function, whose `eval` runs in a namespace
# that cannot see the session's definitions
set @after-chdir = (each {|handler| if (not (and (has-key $handler def) (str:contains $handler[def] "# ${{ function }}\n"))) { put $handler } } $after-chdir)

# The `edit:` module only exists in an interactive session, and referencing it
# directly is a compilation error, which `try` cannot catch
try {
  eval 'use str
set @edit:before-readline = (each {|handler| if (not (and (has-key $handler def) (str:contains $handler[def] "# ${{ function }}\n"))) { put $handler } } $edit:before-readline)'
} catch _ {
  # Nothing was registered when non-interactive
}
