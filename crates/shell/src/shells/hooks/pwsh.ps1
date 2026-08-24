function ${{ activate_function }} {
  $previousExitCode = $global:LASTEXITCODE;
  $exports = ${{ activate_command }};
  if ($exports) {
    $exports | Out-String | Invoke-Expression;
  }
  $global:LASTEXITCODE = $previousExitCode;
}

function ${{ deactivate_function }} {
  $exports = ${{ deactivate_command }};
  if ($exports) {
    $exports | Out-String | Invoke-Expression;
  }

  if (Get-Variable -Name '${{ activate_function }}_handler' -Scope Global -ErrorAction Ignore) {
    $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = [System.Delegate]::Remove(
      $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction,
      $global:${{ activate_function }}_handler
    );
    Remove-Variable -Name '${{ activate_function }}_handler' -Scope Global;
  }

  if (Get-Variable -Name '${{ activate_function }}_prompt' -Scope Global -ErrorAction Ignore) {
    Set-Item -Path 'function:global:prompt' -Value $global:${{ activate_function }}_prompt;
    Remove-Variable -Name '${{ activate_function }}_prompt' -Scope Global;
  }

  Remove-Item -LiteralPath 'function:${{ activate_function }}' -ErrorAction Ignore;
  Remove-Item -LiteralPath 'function:${{ deactivate_function }}' -ErrorAction Ignore;
}

if (-not (Get-Variable -Name '${{ activate_function }}_handler' -Scope Global -ErrorAction Ignore)) {
  $global:${{ activate_function }}_handler = [System.EventHandler[System.Management.Automation.LocationChangedEventArgs]] {
    param([object] $source, [System.Management.Automation.LocationChangedEventArgs] $changedArgs)
    end {
      ${{ activate_function }}
    }
  };

  $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = [System.Delegate]::Combine(
    $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction,
    $global:${{ activate_function }}_handler
  );
};

if (-not (Get-Variable -Name '${{ activate_function }}_prompt' -Scope Global -ErrorAction Ignore)) {
  $global:${{ activate_function }}_prompt = $function:prompt;

  function global:prompt {
    ${{ activate_function }};
    & $global:${{ activate_function }}_prompt;
  }
};
