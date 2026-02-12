#!/bin/bash
# Decapod Automation Setup Script
# Configures local cron/scheduler for Decapod health monitoring

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Decapod Automation Setup"
echo "========================"
echo ""

# Detect OS
OS="$(uname -s)"

# Function to setup cron on Linux/macOS
setup_unix_cron() {
    echo "Setting up cron job for Unix system..."
    
    # Create crontab entry
    CRON_CMD="*/5 * * * * cd $PROJECT_ROOT && $PROJECT_ROOT/scripts/cron-executor.sh >> $PROJECT_ROOT/.decapod/cron.log 2>&1"
    
    # Check if already exists
    if crontab -l 2>/dev/null | grep -q "cron-executor.sh"; then
        echo "✓ Cron job already configured"
    else
        # Add to crontab
        (crontab -l 2>/dev/null; echo "$CRON_CMD") | crontab -
        echo "✓ Added cron job (runs every 5 minutes)"
    fi
    
    echo ""
    echo "Current crontab:"
    crontab -l | grep "decapod" || echo "  (none found)"
}

# Function to setup systemd timer (Linux only)
setup_systemd() {
    if ! command -v systemctl &> /dev/null; then
        return 1
    fi
    
    echo "Setting up systemd timer..."
    
    # Create systemd user service
    mkdir -p ~/.config/systemd/user
    
    cat > ~/.config/systemd/user/decapod-watcher.service <<EOF
[Unit]
Description=Decapod Watcher Service
After=network.target

[Service]
Type=oneshot
WorkingDirectory=$PROJECT_ROOT
ExecStart=$PROJECT_ROOT/scripts/cron-executor.sh
EOF

    cat > ~/.config/systemd/user/decapod-watcher.timer <<EOF
[Unit]
Description=Run Decapod Watcher every 5 minutes

[Timer]
OnBootSec=5min
OnUnitActiveSec=5min

[Install]
WantedBy=timers.target
EOF

    systemctl --user daemon-reload
    systemctl --user enable decapod-watcher.timer
    systemctl --user start decapod-watcher.timer
    
    echo "✓ Systemd timer configured"
    echo "  View status: systemctl --user status decapod-watcher.timer"
    echo "  View logs: journalctl --user -u decapod-watcher.service"
}

# Main setup logic
echo "Project root: $PROJECT_ROOT"
echo ""

case "$OS" in
    Linux*)
        echo "Detected: Linux"
        echo ""
        echo "Choose automation method:"
        echo "1) Cron (traditional, works everywhere)"
        echo "2) Systemd timer (modern, better logging)"
        echo "3) Both"
        echo ""
        read -p "Enter choice [1-3]: " choice
        
        case "$choice" in
            1) setup_unix_cron ;;
            2) setup_systemd ;;
            3) setup_unix_cron && setup_systemd ;;
            *) echo "Invalid choice, defaulting to cron"; setup_unix_cron ;;
        esac
        ;;
    
    Darwin*)
        echo "Detected: macOS"
        echo ""
        setup_unix_cron
        echo ""
        echo "Note: On macOS, you may need to grant cron Full Disk Access"
        echo "      in System Preferences > Security & Privacy > Privacy"
        ;;
    
    *)
        echo "Unknown OS: $OS"
        echo "Attempting cron setup..."
        setup_unix_cron
        ;;
esac

echo ""
echo "========================"
echo "Setup complete!"
echo ""
echo "The watcher will now run automatically every 5 minutes."
echo ""
echo "Manual commands:"
echo "  - Check health:  decapod heartbeat"
echo "  - Run watcher:   decapod watcher run"
echo "  - Run proofs:    decapod proof run"
echo "  - View status:   decapod trust status"
echo ""
echo "Logs location: $PROJECT_ROOT/.decapod/"
echo ""

# Run initial check
echo "Running initial health check..."
cd "$PROJECT_ROOT"
if [ -f "./target/release/decapod" ]; then
    ./target/release/decapod heartbeat
elif [ -f "./target/debug/decapod" ]; then
    ./target/debug/decapod heartbeat
else
    echo "⚠ Decapod binary not found. Build with: cargo build --release"
fi