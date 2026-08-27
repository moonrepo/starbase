if (-not (Get-Variable -Name '${{ function }}_handler' -Scope Global -ErrorAction Ignore)) {
  $global:${{ function }}_handler = [System.EventHandler[System.Management.Automation.LocationChangedEventArgs]] {
    param([object] $source, [System.Management.Automation.LocationChangedEventArgs] $changedArgs)
    end {
      ${{ function }}
    }
  };

  $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = [System.Delegate]::Combine(
    $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction,
    $global:${{ function }}_handler
  );
};

if (-not (Get-Variable -Name '${{ function }}_prompt' -Scope Global -ErrorAction Ignore)) {
  $global:${{ function }}_prompt = $function:prompt;

  function global:prompt {
    ${{ function }};
    & $global:${{ function }}_prompt;
  }
};
