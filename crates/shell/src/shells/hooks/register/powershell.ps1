# Windows PowerShell has no change-dir trigger, since `LocationChangedAction`
# requires PowerShell 6+, so the global prompt is wrapped instead
if (-not (Get-Variable -Name '${{ function }}_on_prompt' -Scope Global -ErrorAction Ignore)) {
  $global:${{ function }}_on_prompt = $function:prompt;

  function global:prompt {
    ${{ function }};
    & $global:${{ function }}_on_prompt;
  }
};
