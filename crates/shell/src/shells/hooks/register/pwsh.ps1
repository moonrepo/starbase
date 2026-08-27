if (-not (Get-Variable -Name '${{ function }}_on_chdir' -Scope Global -ErrorAction Ignore)) {
  $global:${{ function }}_on_chdir = [System.EventHandler[System.Management.Automation.LocationChangedEventArgs]] {
    param([object] $source, [System.Management.Automation.LocationChangedEventArgs] $changedArgs)
    end {
      ${{ function }}
    }
  };

  $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = [System.Delegate]::Combine(
    $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction,
    $global:${{ function }}_on_chdir
  );
};

if (-not (Get-Variable -Name '${{ function }}_on_prompt' -Scope Global -ErrorAction Ignore)) {
  $global:${{ function }}_on_prompt = $function:prompt;

  function global:prompt {
    ${{ function }};
    & $global:${{ function }}_on_prompt;
  }
};
