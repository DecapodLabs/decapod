Output assets/decapod-demo.gif

Set FontSize 20
Set FontFamily "JetBrainsMono Nerd Font Mono"
Set Width 1100
Set Height 630
Set Theme "Dracula"
Set Padding 30
Set BorderRadius 14
Set Margin 20
Set MarginFill "#0b1020"
Set WindowBar Colorful
Set Framerate 30
Set TypingSpeed 40ms
Set Shell "zsh"
Sleep 300ms

Type "printf '\\033[1;38;5;45mDEC\\033[0m\\033[1;38;5;51mAPOD\\033[0m \\033[38;5;213mdemo\\033[0m  |  \\033[38;5;154mopencode + minimax\\033[0m\\n'"
Enter
Sleep 900ms

Type 'opencode run --model opencode/minimax-m2.5-free --dir /tmp/studio "Complete the hello world todo task"
Enter
Sleep 60s

Type "exit"
Enter
