Output assets/decapod-demo.mp4

Set FontSize 20
Set Width 2200
Set Height 1260
Set Theme "Builtin Dark"
Set Padding 24
Set BorderRadius 8
Set TypingSpeed 45ms
Set Shell "zsh"

Type "set -e"
Enter
Sleep 500ms

Type "DEMO_DIR=/tmp/decapod-vhs-demo"
Enter
Type 'rm -rf "$DEMO_DIR" && mkdir -p "$DEMO_DIR" && cd "$DEMO_DIR"'
Enter
Sleep 800ms

Type "decapod init"
Enter
Sleep 3s

Type "tmux kill-session -t vhs-demo 2>/dev/null || true"
Enter
Type 'tmux new-session -d -s vhs-demo -c "$DEMO_DIR"'
Enter
Type 'tmux split-window -h -t vhs-demo:0 -c "$DEMO_DIR"'
Enter
Type "tmux attach -t vhs-demo"
Enter
Sleep 1s

Type "clear"
Enter
Sleep 500ms

Type 'opencode run --dir "$DEMO_DIR" --command '"'"'decapod session acquire >/dev/null 2>&1 || true; decapod todo add --title "VHS demo handoff" --priority medium --type task'"'"' "Create TODO via decapod control plane"'
Enter
Sleep 4s

Type "tmux select-pane -R"
Enter
Sleep 800ms

Type "clear"
Enter
Sleep 500ms

Type 'opencode run --dir "$DEMO_DIR" --command '"'"'decapod session acquire >/dev/null 2>&1 || true; TASK=$(decapod todo list | rg -o "R_[A-Z0-9]+" | head -n1); [ -n "$TASK" ] && decapod todo done "$TASK"'"'"' "Complete TODO via decapod control plane"'
Enter
Sleep 4s

Type "tmux select-pane -L"
Enter
Sleep 800ms

Type "decapod session acquire >/dev/null 2>&1 || true; decapod todo list"
Enter
Sleep 3s

Type "exit"
Enter
