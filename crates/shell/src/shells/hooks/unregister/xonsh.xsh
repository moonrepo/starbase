# A comprehension rather than a loop, so that the names it walks do not outlive
# it in the shell's namespace
[
    event.discard(handler)
    for event in (events.on_chdir, events.on_pre_prompt)
    for handler in list(event)
    if getattr(handler, '__name__', '') == '${{ function }}'
]
