# File Watcher 🎬

A fun, visual command-line tool to watch directory structures and file changes in real-time. Perfect for watching AI agents develop projects!

## Features ✨

- **Real-time monitoring**: Watch files and directories as they change with smooth updates
- **Visual tree view**: See your directory structure in a beautiful, color-coded tree format
- **File type emojis**: 🐍 Python, 📜 JavaScript, 📘 TypeScript, and many more!
- **Change indicators**: ✨ NEW, ✏️  MODIFIED, 🗑️  DELETED with fun emojis
- **Size tracking**: Human-readable file sizes with **delta changes** (e.g., `+1.2KB`, `-500B`)
- **LOC tracking**: Lines of code count with delta changes for text files
- **Development statistics**: Real-time dashboard showing session stats, events per minute, peak file counts
- **Gitignore support**: Automatically respects `.gitignore` patterns
- **Event recording & replay**: Record development sessions and replay them later!

## Installation

### Using uv (recommended)

```bash
# Install uv if you don't have it
curl -LsSf https://astral.sh/uv/install.sh | sh

# Run directly without installation
uv run filewatcher.py

# Or install the tool
uv pip install -e .
```

### Using pip (traditional)

```bash
# Create virtual environment
python3 -m venv venv
source venv/bin/activate

# Install dependencies
pip install -r requirements.txt
```

## Usage

### Basic Usage

```bash
# Watch current directory
./filewatcher

# Watch specific directory
./filewatcher /path/to/directory

# Show hidden files
./filewatcher -a

# Update more frequently (every 0.5 seconds)
./filewatcher -i 0.5
```

### Limiting Output

```bash
# Limit files per directory
./filewatcher -f 10

# Limit directory depth
./filewatcher -d 3

# Combine options
./filewatcher -f 5 -d 2

# Hide statistics dashboard
./filewatcher --no-stats
```

### Recording & Replaying Sessions 🎬

This is the killer feature! Record an AI agent developing a project and replay it later:

```bash
# Record a session (metadata only - small file size)
./filewatcher -r session.json /path/to/project

# Record with file contents (enables content preview/diff in viewer)
./filewatcher -r session.json -c /path/to/project

# Replay the session (shows the evolution over time!)
./filewatcher --replay session.json /path/to/project

# Replay at 2x speed
./filewatcher --replay session.json --replay-speed 2.0 /path/to/project

# Replay at 0.5x speed (slow motion)
./filewatcher --replay session.json --replay-speed 0.5 /path/to/project
```

### Gitignore Support

```bash
# By default, respects .gitignore
./filewatcher

# Show all files including those in .gitignore
./filewatcher --no-gitignore
```

## Controls

- **Ctrl+C**: Stop the watcher or replay

## Command Line Options

- `path` - Directory to watch (default: current directory)
- `-a, --all` - Show hidden files and directories
- `-i INTERVAL, --interval INTERVAL` - Refresh interval in seconds (default: 1.0)
- `-f N, --max-files N` - Maximum files to show per directory
- `-d N, --max-depth N` - Maximum directory depth to display
- `--no-gitignore` - Do not respect .gitignore files
- `--no-stats` - Disable the statistics dashboard
- `-r FILE, --record FILE` - Record events to a JSON file
- `-c, --content` - Include file contents in recording (for content preview/diff)
- `--replay FILE` - Replay events from a JSON file
- `--replay-speed SPEED` - Replay speed multiplier (default: 1.0)

## Example Output

### Standardized UI Layout

The UI uses a consistent table layout with aligned columns:

```
📁 /Users/dylan/myproject

    Name                                    Status     Size     Δ Size  LOC     Δ LOC
───────────────────────────────────────────────────────────────────────────────────────────────
├── 📁 src                                  ✨ NEW                       
│   ├── 🐍 main.py                          ✨ NEW      2.5KB            45L     
│   ├── 🐍 utils.py                         ✏️  MOD    1.2KB    +256B   23L     +5
│   └── 📜 helpers.js                       ✨ NEW      890B             12L     
├── 📁 tests                                ✨ NEW                       
│   └── 🧪 test_main.py                     ✨ NEW      450B             15L     
└── 📝 README.md                            ✏️  MOD    1.5KB    +1.2KB  34L     +12
───────────────────────────────────────────────────────────────────────────────────────────────

📊 4 files  📂 3 dirs  💾 5.09 KB  📄 129L lines   │ ✨ 5 new  ✏️  2 mod

┌──────────────────────────────────────────────────────────────────┐
│                     📈 DEVELOPMENT STATISTICS                     │
├──────────────────────────────────────────────────────────────────┤
│ ⏱️  Session Duration: 45s                                         │
│ ⚡ Activity Rate: 12  events/min                                  │
├──────────────────────────────────────────────────────────────────┤
│ ✨ Created: 5      ✏️  Modified: 2      🗑️  Deleted: 0            │
├──────────────────────────────────────────────────────────────────┤
│ 📁 Files: 4    / 6    peak   📂 Dirs: 3    / 3    peak           │
└──────────────────────────────────────────────────────────────────┘

Legend: ✨ NEW  ✏️  MODIFIED  🗑️  DELETED
```

### Column Explanations

- **Name**: File/directory name with type emoji (🐍 Python, 📜 JS, 📝 MD, etc.)
- **Status**: Change indicator (✨ NEW, ✏️  MOD, 🗑️  DEL)
- **Size**: Current file size (color-coded: dim/cyan/yellow/red based on size)
- **Δ Size**: Size change delta (green = increased, red = decreased, blank = unchanged)
- **LOC**: Lines of code (for text files)
- **Δ LOC**: Line count change delta (green = added, red = removed)

### With File Limits

```
📁 /Users/d/myproject

    Name                                    Status     Size     Δ Size  LOC     Δ LOC
───────────────────────────────────────────────────────────────────────────────────────────────
├── 📁 src                                  ✨ NEW                       
│   ├── 🐍 main.py                          ✨ NEW      2.5KB            45L     
│   ├── 🐍 utils.py                         ✨ NEW      1.2KB            23L     
│   ├── 📜 helpers.js                       ✨ NEW      890B             12L     
│   └── 📝 ... and 12 more file(s)
├── 📁 tests                                ✨ NEW                       
└── 📝 README.md                            ✨ NEW      1.5KB            34L     
───────────────────────────────────────────────────────────────────────────────────────────────

📊 15 files  📂 2 dirs  💾 25.00 KB  📄 450L lines
  ⚠️  Hidden: 12 files
```

### Replay Mode

```bash
$ ./filewatcher --replay session.json .
🎬 Loading replay from: session.json
   📊 47 events over 180.5s
   ✨ 12 created, ✏️  28 modified, 🗑️  7 deleted
   ⏩ Replay speed: 1.0x

[Shows project evolving in real-time with all deltas...]
```

## File Type Emojis

The watcher automatically detects file types and shows appropriate emojis:

- 🐍 Python (.py)
- 📜 JavaScript (.js, .jsx)
- 📘 TypeScript (.ts, .tsx)
- 📋 JSON (.json)
- 📝 Markdown (.md)
- 📄 Text (.txt)
- 🖼️  Images (.png, .jpg, .svg)
- 🎬 Video (.mp4)
- 🎵 Audio (.mp3)
- 📦 Archives (.zip, .tar)
- 💻 Code (other source files)
- ⚙️  Config (.yaml, .yml, .toml)
- 🔀 Git (.gitignore)
- 🔒 Lock files (.lock)
- 🧪 Tests (test files)
- 📚 Documents (.pdf, .doc)
- 📁 Directories

## Development

```bash
# Using uv
uv run python filewatcher.py

# Format code
uv run ruff format filewatcher.py

# Type check
uv run mypy filewatcher.py
```

## How Recording Works

When you use the `-r` flag, the watcher creates a JSON file containing:
- Timestamps of each event
- Event type (created, modified, deleted)
- File path and size
- Directory flag

This allows you to:
1. Record an AI agent's development session
2. Share the recording with others
3. Replay and watch the project evolve step by step
4. Analyze development patterns and timing
5. See exact file size and LOC changes at each step

## UI Design Philosophy

The UI is designed to be:
- **Consistent**: All columns are fixed-width and aligned
- **Informative**: Shows not just current state but also changes (deltas)
- **Visual**: Uses emojis, colors, and box-drawing characters
- **Compact**: Information-dense but readable
- **Engaging**: Makes watching file changes fun!

## License

MIT
