# ChronoCode 🎬

Record and replay file system changes in real-time. Perfect for watching AI agents build projects!

## Features

- **Real-time monitoring**: Watch files and directories as they change
- **Visual tree view**: Color-coded directory structure with file type icons
- **Change tracking**: See created, modified, and deleted files with size/LOC deltas
- **Session recording**: Record development sessions to JSON files
- **Web-based replay viewer**: Replay recordings with timeline scrubbing, content preview, diff view, and code structure analysis
- **Gitignore support**: Automatically respects `.gitignore` patterns

## Installation

### Using uv (recommended)

```bash
# Install uv if you don't have it
curl -LsSf https://astral.sh/uv/install.sh | sh

# Run directly without installation
uv run chronocode.py

# Or install the tool
uv pip install -e .
```

### Using pip

```bash
python3 -m venv venv
source venv/bin/activate
pip install -r requirements.txt
```

## Usage

### Basic Usage

```bash
# Watch current directory
./chronocode

# Watch specific directory
./chronocode /path/to/directory

# Show hidden files
./chronocode -a

# Update more frequently (every 0.5 seconds)
./chronocode -i 0.5
```

### Recording Sessions

```bash
# Record a session (metadata only - small file size)
./chronocode -r

# Record with file contents (enables content preview/diff in viewer)
./chronocode -r -c

# Record to specific file
./chronocode -r session.json /path/to/project
```

### Web Replay Viewer

Open `replay.html` in your browser and drag a recording JSON file onto it.

Features:
- **Timeline scrubber** with event markers
- **Playback controls**: Play/Pause, Step, Speed adjustment
- **File tree** with real-time updates
- **Content preview** (if recorded with `-c`)
- **Diff view** for modified files
- **Structure view** showing functions/classes being worked on

Keyboard shortcuts: `Space` (play/pause), `Arrow keys` (step), `R` (reset), `P` (toggle preview)

### Terminal Replay

```bash
# Replay in terminal
./chronocode --replay session.json /path/to/project

# Replay at 2x speed
./chronocode --replay session.json --replay-speed 2.0 /path/to/project
```

## Command Line Options

| Option | Description |
|--------|-------------|
| `path` | Directory to watch (default: current directory) |
| `-a, --all` | Show hidden files and directories |
| `-i, --interval` | Refresh interval in seconds (default: 1.0) |
| `-f, --max-files` | Maximum files to show per directory |
| `-d, --max-depth` | Maximum directory depth to display |
| `--no-gitignore` | Do not respect .gitignore files |
| `--no-stats` | Disable the statistics dashboard |
| `-r, --record` | Record events to a JSON file |
| `-c, --content` | Include file contents in recording |
| `--replay` | Replay events from a JSON file |
| `--replay-speed` | Replay speed multiplier (default: 1.0) |

## Example Output

```
📁 /Users/user/myproject

    Name                                    Status     Size     Δ Size  LOC     Δ LOC
───────────────────────────────────────────────────────────────────────────────────────
├── 📁 src                                  ✨ NEW                       
│   ├── 🐍 main.py                          ✨ NEW      2.5KB            45L     
│   ├── 🐍 utils.py                         ✏️  MOD    1.2KB    +256B   23L     +5
│   └── 📜 helpers.js                       ✨ NEW      890B             12L     
├── 📁 tests                                ✨ NEW                       
│   └── 🧪 test_main.py                     ✨ NEW      450B             15L     
└── 📝 README.md                            ✏️  MOD    1.5KB    +1.2KB  34L     +12
───────────────────────────────────────────────────────────────────────────────────────

📊 4 files  📂 3 dirs  💾 5.09 KB  📄 129L lines   │ ✨ 5 new  ✏️  2 mod
```

## How Recording Works

When you use the `-r` flag, ChronoCode creates a JSON file containing:
- Timestamps of each event
- Event type (created, modified, deleted)
- File path and size
- Optionally, file contents (with `-c` flag)

Recordings use relative paths, so they're safe to share without leaking your directory structure.

## License

MIT
