# The function may have been defined by a separate `execx()`, whose namespace
# is not the shell's, so it is taken from where the definition exported it
if not any(getattr(handler, '__name__', '') == '${{ function }}' for handler in events.on_chdir):
    events.on_chdir(__xonsh__.ctx['${{ function }}'])

if not any(getattr(handler, '__name__', '') == '${{ function }}' for handler in events.on_pre_prompt):
    events.on_pre_prompt(__xonsh__.ctx['${{ function }}'])
