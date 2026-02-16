Output assets/decapod-demo.gif

Set FontSize 20
Set FontFamily "JetBrainsMono Nerd Font Mono"
Set Width 1100
Set Height 630
Set Theme "Builtin Dark"
Set Padding 24
Set BorderRadius 8
Set TypingSpeed 45ms
Set Shell "zsh"

Type "cd /tmp/decapod-vhs-demo"
Enter
Sleep 400ms

Set TypingSpeed 220ms
Type "decapod init"
Enter
Sleep 3s
Set TypingSpeed 45ms

Type "clear"
Enter
Sleep 500ms

Type 'opencode run --dir /tmp/decapod-vhs-demo "Use decapod control-plane commands to create one TODO titled VHS demo handoff. Acquire session first if needed."'
Enter
Sleep 6s

Type "clear"
Enter
Sleep 500ms

Type 'opencode run --dir /tmp/decapod-vhs-demo "Use decapod control-plane commands to complete the TODO titled VHS demo handoff. Acquire session first if needed."'
Enter
Sleep 6s

Type "decapod session acquire >/dev/null 2>&1 || true; decapod todo list"
Enter
Sleep 3s

Type "exit"
Enter
