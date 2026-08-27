export-env {
    let entry = { code: "${{ function }}" }

    $env.config = ($env.config | upsert hooks.env_change.PWD (
        (($env.config | get --optional hooks.env_change.PWD) | default []) | where { |it| $it != $entry }
    ))

    # Statements the function staged but that have not applied yet are dropped
    # too: the handler may have staged a fresh activation on this very prompt,
    # and left alone it would resurrect the environment right after teardown
    $env.config = ($env.config | upsert hooks.pre_prompt (
        (($env.config | get --optional hooks.pre_prompt) | default []) | where { |it|
            $it != $entry and (not (($it | describe | str starts-with "record") and (($it | get --optional code | default "") | str starts-with "# ${{ function }} apply")))
        }
    ))
}
