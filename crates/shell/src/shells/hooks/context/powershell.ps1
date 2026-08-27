# Windows PowerShell has no change-dir trigger, since `LocationChangedAction`
# requires PowerShell 6+, so the global prompt is wrapped instead
if (-not (Get-Variable -Name '${{ function }}_prompt' -Scope Global -ErrorAction Ignore)) {
  $global:${{ function }}_prompt = $function:prompt;

  function global:prompt {
    ${{ function }};
    & $global:${{ function }}_prompt;
  }
};
