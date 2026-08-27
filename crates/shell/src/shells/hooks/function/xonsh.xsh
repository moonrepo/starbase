def ${{ function }}(olddir=None, newdir=None, **kwargs):
    output = $(${{ command }})
    if output:
        execx(output)

# execx() does not evaluate into the shell namespace, so the function is
# exported for the session, and for the registration, to be able to reach it
__xonsh__.ctx['${{ function }}'] = ${{ function }}
