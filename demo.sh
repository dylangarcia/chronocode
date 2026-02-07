#!/bin/bash
# Demo script to generate a sample filewatcher recording
# This directly creates a JSON file without running filewatcher

set -e

# Create recordings directory if it doesn't exist
mkdir -p recordings

RECORDING="recordings/demo_recording.json"

echo "=============================================="
echo "  File Watcher Demo - Recording Generator"
echo "=============================================="
echo ""

# Generate the demo recording directly as JSON
cat > "$RECORDING" << 'JSONEOF'
{
  "start_time": 1700000000,
  "initial_state": [
    {"path": "demo_project", "size": 0, "is_dir": true}
  ],
  "events": [
    {"timestamp": 0.1, "event_type": "created", "path": "demo_project/src", "size": 0, "is_dir": true},
    {"timestamp": 0.2, "event_type": "created", "path": "demo_project/tests", "size": 0, "is_dir": true},
    {"timestamp": 0.3, "event_type": "created", "path": "demo_project/docs", "size": 0, "is_dir": true},
    {"timestamp": 0.4, "event_type": "created", "path": "demo_project/README.md", "size": 45, "is_dir": false},
    {"timestamp": 0.5, "event_type": "created", "path": "demo_project/src/main.py", "size": 78, "is_dir": false},
    {"timestamp": 0.6, "event_type": "created", "path": "demo_project/config.json", "size": 52, "is_dir": false},
    {"timestamp": 0.7, "event_type": "created", "path": "demo_project/src/utils.py", "size": 32, "is_dir": false},
    {"timestamp": 0.8, "event_type": "created", "path": "demo_project/src/constants.py", "size": 45, "is_dir": false},
    {"timestamp": 0.9, "event_type": "created", "path": "demo_project/tests/test_main.py", "size": 124, "is_dir": false},
    {"timestamp": 1.0, "event_type": "modified", "path": "demo_project/src/main.py", "size": 156, "is_dir": false},
    {"timestamp": 1.1, "event_type": "modified", "path": "demo_project/README.md", "size": 210, "is_dir": false},
    {"timestamp": 1.2, "event_type": "created", "path": "demo_project/requirements.txt", "size": 18, "is_dir": false},
    {"timestamp": 1.3, "event_type": "modified", "path": "demo_project/config.json", "size": 89, "is_dir": false},
    {"timestamp": 1.4, "event_type": "created", "path": "demo_project/docs/getting-started.md", "size": 67, "is_dir": false},
    {"timestamp": 1.5, "event_type": "modified", "path": "demo_project/src/utils.py", "size": 185, "is_dir": false},
    {"timestamp": 1.6, "event_type": "created", "path": "demo_project/tests/test_utils.py", "size": 234, "is_dir": false},
    {"timestamp": 1.7, "event_type": "modified", "path": "demo_project/src/main.py", "size": 312, "is_dir": false},
    {"timestamp": 1.8, "event_type": "deleted", "path": "demo_project/tests/test_main.py", "size": 0, "is_dir": false},
    {"timestamp": 1.9, "event_type": "deleted", "path": "demo_project/docs/getting-started.md", "size": 0, "is_dir": false},
    {"timestamp": 2.0, "event_type": "deleted", "path": "demo_project/docs", "size": 0, "is_dir": true},
    {"timestamp": 2.1, "event_type": "created", "path": "demo_project/LICENSE", "size": 156, "is_dir": false},
    {"timestamp": 2.2, "event_type": "modified", "path": "demo_project/src/constants.py", "size": 89, "is_dir": false},
    {"timestamp": 2.3, "event_type": "modified", "path": "demo_project/README.md", "size": 345, "is_dir": false},
    {"timestamp": 2.4, "event_type": "created", "path": "demo_project/src/api.py", "size": 234, "is_dir": false},
    {"timestamp": 2.5, "event_type": "modified", "path": "demo_project/src/main.py", "size": 456, "is_dir": false}
  ]
}
JSONEOF

echo "Recording saved to: $RECORDING"
echo ""
echo "Events: 25"
echo "Initial files: 1"
echo ""
echo "To view the recording:"
echo "  1. Open replay.html in your browser"
echo "  2. Drop the $RECORDING file onto the page"
echo "  3. Use Space to play/pause, arrow keys to step"
echo ""

# Optionally open the viewer
if command -v open &> /dev/null; then
    read -p "Open replay.html in browser? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        open replay.html
    fi
fi
