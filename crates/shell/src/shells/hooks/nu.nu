export def --env ${{ activate_function }} [] {
    # Staged for the `source` entry that runs next. A failing command stages
    # nothing rather than aborting the prompt, and its stderr is left alone.
    let file = ${{ file }}

    try { ${{ activate_command }} | save --force $file } catch { "" | save --force $file }
}

export def --env ${{ deactivate_function }} [] {
    # The statements are shell syntax and no command can evaluate them, so the
    # reversal is staged the same way, and lands on the next trigger rather
    # than immediately. Its last act is to unregister the `source` entry.
    let file = ${{ file }}

    try { ${{ deactivate_command }} | save --force $file } catch { "" | save --force $file }

    ["" '${{ cleanup }}'] | str join (char newline) | save --append $file

    # The writer entry goes now, so that it cannot overwrite the staged
    # teardown before the `source` entry has applied it.
${{ unregister }}}

export-env {
    # The `source` entry parses this file before running it, so it must exist
    # ahead of the first trigger. It removes it again once applied, so this
    # only has to cover the window before the first one.
    let file = ${{ file }}

    if not ($file | path exists) { "" | save --force $file }

    let entries = [
        { code: "${{ activate_function }}" }
        { code: '${{ source_entry }}' }
    ]
${{ register }}}
