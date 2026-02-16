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
Sleep 300ms

Set TypingSpeed 120ms
Type "decapod init"
Enter
Sleep 3s
Set TypingSpeed 45ms

Type 'opencode run --model opencode/minimax-m2.5-free --dir /tmp/codebase "Use decapod control-plane commands to complete the existing TODO titled VHS demo handoff. Acquire session first if needed."'
Enter
Sleep 6s

Type "exit"
Enter
