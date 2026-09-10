export def --env ${{ function }} [] {
    # Nushell has no runtime `eval`, but `source` parses and runs a file in the
    # scope that invoked it, which is enough to apply a statement list. Only a
    # hook entry defined as a string is parsed that way, and it is parsed each
    # time it runs, so the statements are written out and an entry is kept in
    # `pre_prompt` that sources the file and then empties it. The entry runs
    # after this function on every prompt, so the statements are applied on
    # the prompt after they are written, and an empty file is a no-op.
    let file = ${{ file }}

    # A failing command stages nothing rather than aborting the prompt, and its
    # stderr is left alone
    try { ${{ command }} | save --force $file } catch { "" | save --force $file }

    # Other entries may be strings, closures, or records holding either, so the
    # entry is found by whole record equality, which is false across types
    let apply = ${{ apply }}

    $env.config = ($env.config | upsert hooks.pre_prompt { |config|
        let list = ($config | get --optional hooks.pre_prompt) | default []

        if $apply in $list { $list } else { $list | append $apply }
    })
}
