export-env {
    let entry = { code: "${{ function }}" }

    $env.config = ($env.config | upsert hooks.env_change.PWD { |config|
        let list = ($config | get --optional hooks.env_change.PWD) | default []

        if $entry in $list { $list } else { $list | append $entry }
    })

    $env.config = ($env.config | upsert hooks.pre_prompt { |config|
        let list = ($config | get --optional hooks.pre_prompt) | default []

        if $entry in $list { $list } else { $list | append $entry }
    })
}
