export-env {
    let entry = { code: "${{ function }}" }
    let apply = ${{ apply }}

    $env.config = ($env.config | upsert hooks.env_change.PWD (
        (($env.config | get --optional hooks.env_change.PWD) | default []) | where { |it| $it != $entry }
    ))

    # The entry that applies the function's statements goes too: the handler
    # may have written a fresh activation on this very prompt, and left alone
    # it would resurrect the environment right after teardown
    $env.config = ($env.config | upsert hooks.pre_prompt (
        (($env.config | get --optional hooks.pre_prompt) | default []) | where { |it| $it != $entry and $it != $apply }
    ))
}
