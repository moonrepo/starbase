if (Get-Variable -Name '${{ function }}_on_prompt' -Scope Global -ErrorAction Ignore) {
  Set-Item -Path 'function:global:prompt' -Value $global:${{ function }}_prompt;
  Remove-Variable -Name '${{ function }}_on_prompt' -Scope Global;
}
