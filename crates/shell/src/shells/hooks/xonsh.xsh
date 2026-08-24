def ${{ activate_function }}(olddir=None, newdir=None, **kwargs):
    output = $(${{ activate_command }})
    if output:
        execx(output)

def ${{ deactivate_function }}():
    output = $(${{ deactivate_command }})
    if output:
        execx(output)
    for event in (events.on_chdir, events.on_pre_prompt):
        for handler in list(event):
            if getattr(handler, '__name__', '') == '${{ activate_function }}':
                event.discard(handler)
    __xonsh__.ctx.pop('${{ activate_function }}', None)
    __xonsh__.ctx.pop('${{ deactivate_function }}', None)

# Re-sourcing creates new function objects, so deduplicate by name
if not any(getattr(handler, '__name__', '') == '${{ activate_function }}' for handler in events.on_chdir):
    events.on_chdir(${{ activate_function }})

if not any(getattr(handler, '__name__', '') == '${{ activate_function }}' for handler in events.on_pre_prompt):
    events.on_pre_prompt(${{ activate_function }})

# execx() does not evaluate into the shell namespace, so export both functions
__xonsh__.ctx['${{ activate_function }}'] = ${{ activate_function }}
__xonsh__.ctx['${{ deactivate_function }}'] = ${{ deactivate_function }}
