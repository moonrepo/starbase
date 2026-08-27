if (Get-Variable -Name '${{ function }}_on_chdir' -Scope Global -ErrorAction Ignore) {
  $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = [System.Delegate]::Remove(
    $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction,
    $global:${{ function }}_on_chdir
  );
  Remove-Variable -Name '${{ function }}_on_chdir' -Scope Global;
}

if (Get-Variable -Name '${{ function }}_on_prompt' -Scope Global -ErrorAction Ignore) {
  Set-Item -Path 'function:global:prompt' -Value $global:${{ function }}_on_prompt;
  Remove-Variable -Name '${{ function }}_on_prompt' -Scope Global;
}
