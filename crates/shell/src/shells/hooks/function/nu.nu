export def --env ${{ function }} [] {
    # Nushell has no runtime `eval`, but `source` parses and runs a file in the
    # scope that invoked it, which is enough to apply a statement list. Only a
    # hook entry defined as a string is parsed that way, so the statements are
    # written out and a one shot entry is staged to apply them, which it does
    # before the next prompt and then removes itself.
    #
    # `source` requires a parse time constant path (a runtime path fails with
    # `nu::shell::not_a_constant`), so the name is baked into the staged entry.
    # It is keyed by pid, so concurrent sessions cannot read each other's
    # statements, and by function, so two tools cannot clobber each other.
    let file = ${{ file }}

    # A failing command stages nothing rather than aborting the prompt, and its
    # stderr is left alone
    try { ${{ command }} | save --force $file } catch { "" | save --force $file }

    # An entry staged by an earlier call is replaced rather than duplicated.
    # Other entries may be strings, closures, or records holding either, and
    # are left alone
    let entries = ((($env.config | get --optional hooks.pre_prompt) | default [])
        | where ${{ filter }})

    $env.config = ($env.config | upsert hooks.pre_prompt ($entries | append { code: ${{ staged }} }))
}
