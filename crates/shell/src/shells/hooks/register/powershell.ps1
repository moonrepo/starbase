# Windows PowerShell has no change-dir trigger, since `LocationChangedAction`
# requires PowerShell 6+, so the global prompt is wrapped instead
if (-not (Get-Variable -Name '${{ function }}_on_prompt' -Scope Global -ErrorAction Ignore)) {
  $global:${{ function }}_on_prompt = $function:prompt;

  # A session started without a prompt (`-File`, `-Command`) has nothing to
  # save: fall back to the engine's default, so the wrapper never invokes
  # `$null` and unregistering always restores a callable prompt
  if ($null -eq $global:${{ function }}_on_prompt) {
    $global:${{ function }}_on_prompt = { "PS $($executionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) " };
  }

  function global:prompt {
    ${{ function }};
    & $global:${{ function }}_on_prompt;
  }
};
