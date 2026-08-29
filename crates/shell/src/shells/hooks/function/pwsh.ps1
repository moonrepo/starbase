function ${{ function }} {
  $previousExitCode = $global:LASTEXITCODE;
  $exports = ${{ command }};
  if ($exports) {
    $exports | Out-String | Invoke-Expression;
  }
  $global:LASTEXITCODE = $previousExitCode;
}
