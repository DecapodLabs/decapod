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

Type 'opencode run --dir "$DEMO_DIR" "Use decapod control-plane commands to create one TODO task titled VHS demo handoff. If a session token is required, run decapod session acquire first."'
Enter
Sleep 10s

Type "tmux select-pane -R"
Enter
Sleep 800ms

Type "clear"
Enter
Sleep 500ms

Type 'opencode run --dir "$DEMO_DIR" "Use decapod control-plane commands to find the open TODO titled VHS demo handoff and mark it done. If a session token is required, run decapod session acquire first."'
Enter
Sleep 10s

Type "tmux select-pane -L"
Enter
Sleep 800ms

Type "decapod todo list"
Enter
Sleep 3s

Type "exit"
Enter
