export-env {
    let entry = { code: "${{ function }}" }

    $env.config = ($env.config | upsert hooks.env_change.PWD (
        (($env.config | get --optional hooks.env_change.PWD) | default []) | where { |it| $it != $entry }
    ))

    $env.config = ($env.config | upsert hooks.pre_prompt (
        (($env.config | get --optional hooks.pre_prompt) | default []) | where { |it| $it != $entry }
    ))
}
